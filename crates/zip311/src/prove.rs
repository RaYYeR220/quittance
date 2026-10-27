//! Creating a disclosure (the sender's side).

use orchard::{
    builder::{Builder, BundleType},
    bundle::{Authorized, BundleVersion, Flags},
    circuit::ProvingKey,
    keys::{FullViewingKey, OutgoingViewingKey, Scope, SpendAuthorizingKey},
    note::ExtractedNoteCommitment,
    note_encryption::IronwoodDomain,
    tree::MerklePath,
    Bundle, Note,
};
use rand::{CryptoRng, Rng};
use zcash_note_encryption::{try_output_recovery_with_ock, Domain, EphemeralKeyBytes};
use zcash_primitives::transaction::Transaction;

use crate::error::ProveError;
use crate::format::{self, OutputDisclosure, PaymentDisclosure, ReprovedAction, ReprovedSpends, MAX_MSG};
use crate::network::Network;

/// A note the transaction spent, with its Merkle path at the transaction's anchor.
#[derive(Clone, Debug)]
pub struct SpendWitness {
    pub note: Note,
    pub merkle_path: MerklePath,
}

/// Which outputs to open.
#[derive(Clone, Debug)]
pub enum OutputSelection {
    /// Every output the wallet's external outgoing viewing key opens: the
    /// payments, not the change.
    Payments,
    /// Exactly these action indices.
    Indices(Vec<u32>),
}

pub struct ProveRequest<'a> {
    pub network: Network,
    pub tx: &'a Transaction,
    /// The height the transaction was mined at.
    pub height: u32,
    /// The verifier's challenge.
    pub msg: Vec<u8>,
    pub fvk: &'a FullViewingKey,
    pub ask: &'a SpendAuthorizingKey,
    pub spends: Vec<SpendWitness>,
    pub outputs: OutputSelection,
    /// Attach the raw transaction so the verifier needn't look it up by txid.
    pub embed_tx: bool,
}

/// Creates a disclosure whose spends are re-proven under the transaction's anchor.
pub fn prove_reproved<R: Rng + CryptoRng>(
    req: ProveRequest<'_>,
    pk: &ProvingKey,
    rng: &mut R,
) -> Result<PaymentDisclosure, ProveError> {
    let bundle = req.tx.ironwood_bundle().ok_or(ProveError::NoIronwoodBundle)?;
    let txid: [u8; 32] = *req.tx.txid().as_ref();
    let mut d = prove_bundle(req.network, txid, bundle, req.height, req.msg, req.fvk, req.ask, &req.spends, &req.outputs, pk, rng)?;
    if req.embed_tx {
        let mut raw = vec![];
        req.tx.write(&mut raw)?;
        d.tx = Some(raw);
    }
    Ok(d)
}

/// The same as [`prove_reproved`], over a bare Ironwood bundle and txid.
#[allow(clippy::too_many_arguments)]
pub fn prove_bundle<V, R: Rng + CryptoRng>(
    network: Network,
    txid: [u8; 32],
    bundle: &Bundle<Authorized, V>,
    height: u32,
    msg: Vec<u8>,
    fvk: &FullViewingKey,
    ask: &SpendAuthorizingKey,
    spends: &[SpendWitness],
    outputs: &OutputSelection,
    pk: &ProvingKey,
    rng: &mut R,
) -> Result<PaymentDisclosure, ProveError> {
    if msg.len() > MAX_MSG {
        return Err(ProveError::MessageTooLong(MAX_MSG));
    }
    if spends.is_empty() {
        return Err(ProveError::NoSpends);
    }
    let actions = bundle.actions();
    let anchor = *bundle.anchor();

    // Each witness must be for a note this transaction spent, at its anchor.
    for (i, w) in spends.iter().enumerate() {
        let nf = w.note.nullifier(fvk);
        if !actions.iter().any(|a| *a.nullifier() == nf) {
            return Err(ProveError::NoteNotSpentHere(i));
        }
        let cmx = ExtractedNoteCommitment::from(w.note.commitment());
        if w.merkle_path.root(cmx) != anchor {
            return Err(ProveError::WrongAnchor(i));
        }
    }

    // Open the requested outputs with the wallet's outgoing viewing keys.
    let ovks = [fvk.to_ovk(Scope::External), fvk.to_ovk(Scope::Internal)];
    let opened = |i: usize, ovk: &OutgoingViewingKey| -> Option<[u8; 32]> {
        let a = &actions[i];
        let ock = IronwoodDomain::derive_ock(ovk, a.cv_net(), &a.cmx().to_bytes(), &EphemeralKeyBytes(a.encrypted_note().epk_bytes));
        try_output_recovery_with_ock(&IronwoodDomain::for_action(a), &ock, a, &a.encrypted_note().out_ciphertext)
            .map(|_| ock.0)
    };
    let mut disclosed_outputs = vec![];
    match outputs {
        OutputSelection::Payments => {
            for i in 0..actions.len() {
                if let Some(ock) = opened(i, &ovks[0]) {
                    disclosed_outputs.push(OutputDisclosure { tx_action_index: i as u32, ock });
                }
            }
            if disclosed_outputs.is_empty() {
                return Err(ProveError::NoPayments);
            }
        }
        OutputSelection::Indices(indices) => {
            for &i in indices {
                if i as usize >= actions.len() {
                    return Err(ProveError::OutputOutOfRange(i));
                }
                let ock = ovks.iter().find_map(|ovk| opened(i as usize, ovk)).ok_or(ProveError::NotOurOutput(i))?;
                disclosed_outputs.push(OutputDisclosure { tx_action_index: i, ock });
            }
        }
    }

    // The re-proof: an Ironwood bundle spending the same notes under the same
    // anchor, outputs disabled so every output it carries is worth zero.
    let mut builder = Builder::new(BundleType::UNPADDED, BundleVersion::ironwood_v3(), Flags::OUTPUTS_DISABLED, anchor)
        .map_err(|e| ProveError::Build(format!("{e:?}")))?;
    for w in spends {
        builder
            .add_spend(fvk.clone(), w.note.clone(), w.merkle_path.clone())
            .map_err(|e| ProveError::Build(format!("{e:?}")))?;
    }
    let (unproven, _) = builder
        .build::<i64>(&mut *rng)
        .map_err(|e| ProveError::Build(format!("{e:?}")))?
        .ok_or_else(|| ProveError::Build("empty bundle".into()))?;
    let proven = unproven.create_proof(pk, &mut *rng).map_err(|e| ProveError::Build(format!("{e:?}")))?;

    // Tie every re-proof action to the transaction action that spent the same note.
    let mut parts = vec![];
    for a in proven.actions() {
        let index = actions
            .iter()
            .position(|t| t.nullifier() == a.nullifier())
            .ok_or_else(|| ProveError::Build("a re-proof action spends no note of this transaction".into()))?;
        parts.push((index as u32, a.cv_net().to_bytes(), <[u8; 32]>::from(a.rk()), a.cmx().to_bytes()));
    }
    let outs: Vec<(u32, [u8; 32])> = disclosed_outputs.iter().map(|o| (o.tx_action_index, o.ock)).collect();
    let unsigned = format::unsigned_bytes(network, &txid, height, &msg, &parts, &[], &outs);
    let digest = format::digest(network, &unsigned);

    let signed = proven
        .prepare(&mut *rng, digest)
        .sign(&mut *rng, ask)
        .finalize()
        .map_err(|e| ProveError::Build(format!("{e:?}")))?;
    let proof = signed.authorization().proof().as_ref().to_vec();
    let reproved = parts
        .iter()
        .zip(signed.actions().iter())
        .map(|(&(tx_action_index, cv_net, rk, cmx), a)| ReprovedAction {
            tx_action_index,
            cv_net,
            rk,
            cmx,
            spend_auth_sig: <[u8; 64]>::from(a.authorization()),
        })
        .collect();

    Ok(PaymentDisclosure {
        network,
        txid,
        height,
        msg,
        reproved: Some(ReprovedSpends { actions: reproved, proof }),
        resigned: vec![],
        outputs: disclosed_outputs,
        tx: None,
    })
}
