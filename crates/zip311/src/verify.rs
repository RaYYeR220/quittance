//! Checking a disclosure against the chain (the verifier's side).
//!
//! Every step is reported as a named check, so a refusal always says which
//! claim failed and why, never just "invalid".

use std::collections::HashSet;

use orchard::{
    bundle::{Authorized, Flags},
    circuit::{Instance, Proof, VerifyingKey},
    note::ExtractedNoteCommitment,
    note_encryption::IronwoodDomain,
    primitives::redpallas::{self, SpendAuth},
    value::ValueCommitment,
    Bundle,
};
use serde::Serialize;
use zcash_note_encryption::{try_output_recovery_with_ock, OutgoingCipherKey};
use zcash_primitives::transaction::Transaction;

use crate::challenge::Challenge;
use crate::format::PaymentDisclosure;
use crate::network::Network;

pub struct VerifyContext<'a> {
    /// The network the verifier is on; a disclosure for another network is refused.
    pub network: Network,
    /// The challenge the verifier issued. `None` checks everything else and
    /// reports the disclosure as not bound to a challenge of yours.
    pub expected_msg: Option<&'a [u8]>,
    pub vk: &'a VerifyingKey,
    /// Unix time, for challenge expiry.
    pub now: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Fail,
    /// Not a failure of the disclosure, but something the verifier must know.
    Warn,
    Skip,
}

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub label: &'static str,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum MemoView {
    Empty,
    Text(String),
    Bytes(String),
}

#[derive(Clone, Debug, Serialize)]
pub struct Payment {
    pub action_index: u32,
    pub value_zat: u64,
    pub value_zec: String,
    /// The recipient as a unified address with a single Ironwood/Orchard receiver.
    pub recipient: String,
    pub memo: MemoView,
}

#[derive(Clone, Debug, Serialize)]
pub struct SpendInfo {
    pub action_index: u32,
    pub nullifier: String,
    pub mode: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct Revealed {
    pub payments: Vec<Payment>,
    pub spends: Vec<SpendInfo>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Verdict {
    pub valid: bool,
    pub network: Network,
    pub txid: String,
    pub height: u32,
    pub challenge_text: Option<String>,
    pub challenge: Option<Challenge>,
    pub checks: Vec<Check>,
    /// What the disclosure proves, present only when it is valid.
    pub revealed: Option<Revealed>,
}

/// What a disclosure never reveals; shown next to every verdict.
pub const NOT_REVEALED: &[&str] = &[
    "the sender's address or viewing key",
    "the value of the notes the sender spent",
    "the sender's change and every output not disclosed",
    "any other transaction of the sender or the recipient",
];

struct Report {
    checks: Vec<Check>,
}

impl Report {
    fn push(&mut self, id: &'static str, label: &'static str, status: CheckStatus, detail: impl Into<String>) -> bool {
        self.checks.push(Check { id, label, status, detail: detail.into() });
        status != CheckStatus::Fail
    }
    fn pass(&mut self, id: &'static str, label: &'static str, detail: impl Into<String>) -> bool {
        self.push(id, label, CheckStatus::Pass, detail)
    }
    fn fail(&mut self, id: &'static str, label: &'static str, detail: impl Into<String>) -> bool {
        self.push(id, label, CheckStatus::Fail, detail)
    }
}

/// Verifies `d` against `tx` as found on chain at `mined_height` (`None` if the
/// chain source has no mined copy of it).
pub fn verify(d: &PaymentDisclosure, tx: &Transaction, mined_height: Option<u32>, cx: &VerifyContext<'_>) -> Verdict {
    let txid: [u8; 32] = *tx.txid().as_ref();
    verify_bundle(d, txid, tx.ironwood_bundle(), tx.orchard_bundle().is_some(), mined_height, cx)
}

/// The same as [`verify`], over a txid and the transaction's shielded bundles.
pub fn verify_bundle<V>(
    d: &PaymentDisclosure,
    txid: [u8; 32],
    ironwood: Option<&Bundle<Authorized, V>>,
    has_orchard: bool,
    mined_height: Option<u32>,
    cx: &VerifyContext<'_>,
) -> Verdict {
    let mut r = Report { checks: vec![] };
    let challenge = Challenge::parse(&d.msg);
    let ok = run(d, txid, ironwood, has_orchard, mined_height, cx, &challenge, &mut r);
    let revealed = ok.then(|| reveal(d, ironwood.expect("checked"), cx.network));
    let valid = revealed.is_some() && r.checks.iter().all(|c| c.status != CheckStatus::Fail);
    Verdict {
        valid,
        network: d.network,
        txid: d.txid_display(),
        height: d.height,
        challenge_text: std::str::from_utf8(&d.msg).ok().map(str::to_owned),
        challenge,
        checks: r.checks,
        revealed: if valid { revealed } else { None },
    }
}

#[allow(clippy::too_many_arguments)]
fn run<V>(
    d: &PaymentDisclosure,
    txid: [u8; 32],
    ironwood: Option<&Bundle<Authorized, V>>,
    has_orchard: bool,
    mined_height: Option<u32>,
    cx: &VerifyContext<'_>,
    challenge: &Option<Challenge>,
    r: &mut Report,
) -> bool {
    if d.network != cx.network {
        return r.fail("network", "Network", format!("the disclosure is for {}, this verifier is on {}", d.network, cx.network));
    }
    r.pass("network", "Network", d.network.to_string());

    if txid != d.txid {
        return r.fail("txid", "Transaction", "the transaction supplied does not hash to the disclosed txid");
    }
    r.pass("txid", "Transaction", format!("txid {}", d.txid_display()));

    match mined_height {
        None => return r.fail("mined", "Mined", "the chain has no mined transaction with this txid"),
        Some(h) if h != d.height => {
            return r.fail("mined", "Mined", format!("mined at height {h}, but the disclosure claims {}", d.height))
        }
        Some(h) => r.pass("mined", "Mined", format!("height {h}")),
    };

    let Some(bundle) = ironwood else {
        let why = if has_orchard {
            "the transaction spends from the Orchard pool. Orchard proofs are not accepted as evidence: the Orchard circuit had a soundness bug (fixed by NU6.2/NU6.3 in 2026), so a proof under it doesn't show that a note was really spent"
        } else {
            "the transaction has no Ironwood bundle; only Ironwood payments can be disclosed"
        };
        return r.fail("pool", "Ironwood pool", why);
    };
    r.pass("pool", "Ironwood pool", format!("{} actions", bundle.actions().len()));
    let actions = bundle.actions();

    // Indices: in range and not repeated.
    let reproved = d.reproved.as_ref().map(|s| s.actions.as_slice()).unwrap_or(&[]);
    let spend_idx: Vec<u32> = reproved.iter().map(|a| a.tx_action_index).chain(d.resigned.iter().map(|s| s.tx_action_index)).collect();
    if spend_idx.is_empty() {
        return r.fail("spend", "Sender", "the disclosure proves no spend, so it says nothing about who paid");
    }
    let out_idx: Vec<u32> = d.outputs.iter().map(|o| o.tx_action_index).collect();
    for (what, list) in [("spend", &spend_idx), ("output", &out_idx)] {
        let mut seen = HashSet::new();
        for &i in list.iter() {
            if i as usize >= actions.len() {
                return r.fail("indices", "Action indices", format!("{what} action {i} is out of range ({} actions)", actions.len()));
            }
            if !seen.insert(i) {
                return r.fail("indices", "Action indices", format!("{what} action {i} is listed twice"));
            }
        }
    }
    r.pass("indices", "Action indices", format!("spends {spend_idx:?}, outputs {out_idx:?}"));

    let digest = d.digest();

    // Re-proven spends: one Halo2 proof over every re-proof action, against the
    // transaction's own anchor and the nullifiers its actions publish.
    if let Some(rs) = &d.reproved {
        let mut instances = Vec::with_capacity(rs.actions.len());
        let mut keys = Vec::with_capacity(rs.actions.len());
        for a in &rs.actions {
            let t = &actions[a.tx_action_index as usize];
            let parsed = (|| {
                let cv = Option::from(ValueCommitment::from_bytes(&a.cv_net))?;
                let rk = redpallas::VerificationKey::<SpendAuth>::try_from(a.rk).ok()?;
                let cmx = Option::from(ExtractedNoteCommitment::from_bytes(&a.cmx))?;
                Some((cv, rk, cmx))
            })();
            let Some((cv, rk, cmx)) = parsed else {
                return r.fail("spend-proof", "Sender: spend re-proven", format!("re-proof for action {} has a malformed public input", a.tx_action_index));
            };
            let Some(inst) = Instance::from_parts(*bundle.anchor(), cv, *t.nullifier(), rk.clone(), cmx, Flags::OUTPUTS_DISABLED) else {
                return r.fail("spend-proof", "Sender: spend re-proven", "identity rk");
            };
            instances.push(inst);
            keys.push((a.tx_action_index, rk, redpallas::Signature::<SpendAuth>::from(a.spend_auth_sig)));
        }
        if let Err(e) = Proof::new(rs.proof.clone()).verify(cx.vk, &instances) {
            return r.fail(
                "spend-proof",
                "Sender: spend re-proven",
                format!("the proof does not show knowledge of the notes behind nullifiers at actions {:?} under the transaction's anchor ({e:?})", keys.iter().map(|k| k.0).collect::<Vec<_>>()),
            );
        }
        r.pass("spend-proof", "Sender: spend re-proven", format!("Halo2 proof over {} action(s), under the transaction's anchor", instances.len()));
        for (i, rk, sig) in &keys {
            if rk.verify(&digest, sig).is_err() {
                return r.fail("spend-auth", "Sender: signed this disclosure", format!("the spend authorization signature for action {i} does not cover this disclosure: a different challenge, txid or output set was signed"));
            }
        }
    }
    // Re-signed spends: a signature under the transaction's own rk.
    for s in &d.resigned {
        let t = &actions[s.tx_action_index as usize];
        let sig = redpallas::Signature::<SpendAuth>::from(s.spend_auth_sig);
        if t.rk().verify(&digest, &sig).is_err() {
            return r.fail("spend-auth", "Sender: signed this disclosure", format!("the signature for action {} does not verify under that action's spend key", s.tx_action_index));
        }
    }
    r.pass("spend-auth", "Sender: signed this disclosure", format!("{} spend authorization signature(s) over the disclosure digest", spend_idx.len()));

    // Challenge binding.
    match cx.expected_msg {
        Some(expected) if expected == d.msg.as_slice() => {
            r.pass("challenge", "Bound to your challenge", "signed over exactly the challenge you issued");
        }
        Some(_) => {
            return r.fail("challenge", "Bound to your challenge", "signed over a different challenge: this disclosure was made for someone else, or replayed");
        }
        None => {
            r.push("challenge", "Bound to your challenge", CheckStatus::Warn, "no challenge of yours was checked, so this could be a replay of a disclosure made for someone else");
        }
    }
    if let Some(c) = challenge {
        if c.is_expired(cx.now) {
            r.push("expiry", "Challenge fresh", CheckStatus::Warn, format!("the challenge expired at unix time {}", c.expires));
        } else {
            r.pass("expiry", "Challenge fresh", format!("valid until unix time {}", c.expires));
        }
    }

    // Outputs.
    if d.outputs.is_empty() {
        r.push("outputs", "Payment opened", CheckStatus::Warn, "no output is disclosed: this proves who spent, not whom they paid");
        return true;
    }
    for o in &d.outputs {
        let a = &actions[o.tx_action_index as usize];
        let opened = try_output_recovery_with_ock(&IronwoodDomain::for_action(a), &OutgoingCipherKey(o.ock), a, &a.encrypted_note().out_ciphertext);
        if opened.is_none() {
            return r.fail("outputs", "Payment opened", format!("the key given for action {} does not open it", o.tx_action_index));
        }
    }
    r.pass("outputs", "Payment opened", format!("{} output(s) opened; note commitments match", d.outputs.len()))
}

fn reveal<V>(d: &PaymentDisclosure, bundle: &Bundle<Authorized, V>, network: Network) -> Revealed {
    let actions = bundle.actions();
    let payments = d
        .outputs
        .iter()
        .filter_map(|o| {
            let a = &actions[o.tx_action_index as usize];
            let (note, to, memo) = try_output_recovery_with_ock(&IronwoodDomain::for_action(a), &OutgoingCipherKey(o.ock), a, &a.encrypted_note().out_ciphertext)?;
            let value = note.value().inner();
            Some(Payment {
                action_index: o.tx_action_index,
                value_zat: value,
                value_zec: format_zec(value),
                recipient: encode_recipient(&to, network),
                memo: memo_view(&memo),
            })
        })
        .collect();
    let mut spends: Vec<SpendInfo> = d
        .reproved
        .iter()
        .flat_map(|r| r.actions.iter())
        .map(|a| SpendInfo { action_index: a.tx_action_index, nullifier: hex::encode(actions[a.tx_action_index as usize].nullifier().to_bytes()), mode: "reproved" })
        .collect();
    spends.extend(d.resigned.iter().map(|s| SpendInfo {
        action_index: s.tx_action_index,
        nullifier: hex::encode(actions[s.tx_action_index as usize].nullifier().to_bytes()),
        mode: "resigned",
    }));
    Revealed { payments, spends }
}

pub(crate) fn format_zec(zat: u64) -> String {
    let s = format!("{}.{:08}", zat / 100_000_000, zat % 100_000_000);
    let s = s.trim_end_matches('0');
    s.strip_suffix('.').map(|x| format!("{x}.0")).unwrap_or_else(|| s.to_string())
}

fn encode_recipient(to: &orchard::Address, network: Network) -> String {
    use zcash_address::unified::{self, Encoding};
    let ua = unified::Address::try_from_items(unified::Revision::R0, vec![unified::Uitem::Data(unified::Receiver::Orchard(to.to_raw_address_bytes()))])
        .expect("a single Orchard receiver is a valid unified address");
    ua.encode(&network.network_type())
}

/// ZIP 302: a first byte up to 0xF4 is UTF-8 text padded with zeros; 0xF6 followed
/// by zeros is "no memo"; anything else is shown as bytes.
fn memo_view(memo: &[u8; 512]) -> MemoView {
    if memo[0] == 0xF6 && memo[1..].iter().all(|b| *b == 0) {
        return MemoView::Empty;
    }
    if memo[0] <= 0xF4 {
        let end = memo.iter().rposition(|b| *b != 0).map(|p| p + 1).unwrap_or(0);
        if end == 0 {
            return MemoView::Empty;
        }
        if let Ok(s) = std::str::from_utf8(&memo[..end]) {
            return MemoView::Text(s.to_string());
        }
    }
    let end = memo.iter().rposition(|b| *b != 0).map(|p| p + 1).unwrap_or(1);
    MemoView::Bytes(hex::encode(&memo[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zec_formatting() {
        assert_eq!(format_zec(0), "0.0");
        assert_eq!(format_zec(30_000), "0.0003");
        assert_eq!(format_zec(100_000_000), "1.0");
        assert_eq!(format_zec(123_456_789), "1.23456789");
    }

    #[test]
    fn memo_views() {
        let mut m = [0u8; 512];
        assert!(matches!(memo_view(&m), MemoView::Empty));
        m[0] = 0xF6;
        assert!(matches!(memo_view(&m), MemoView::Empty));
        let mut t = [0u8; 512];
        t[..5].copy_from_slice(b"hello");
        assert!(matches!(memo_view(&t), MemoView::Text(ref s) if s == "hello"));
        let mut b = [0u8; 512];
        b[0] = 0xFF;
        b[1] = 0x01;
        assert!(matches!(memo_view(&b), MemoView::Bytes(ref s) if s == "ff01"));
    }
}
