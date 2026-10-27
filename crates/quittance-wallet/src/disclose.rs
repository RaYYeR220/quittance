//! Capturing disclosure kits and answering challenges with them.

use std::collections::HashSet;

use incrementalmerkletree::Position;
use orchard::{
    keys::{FullViewingKey, SpendAuthorizingKey},
    Anchor, Note,
};
use rand::{rand_core::UnwrapErr, rngs::SysRng};
use shardtree::error::ShardTreeError;
use zcash_client_backend::data_api::{WalletCommitmentTrees, WalletRead};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::{consensus::BlockHeight, TxId};
use zip311::{OutputSelection, PaymentDisclosure, ProveRequest, SpendWitness};

use crate::error::{Error, Result};
use crate::kit::DisclosureKit;
use crate::lightwalletd::Lightwalletd;
use crate::wallet::{Db, Wallet};

type TreeError = ShardTreeError<<Db as WalletCommitmentTrees>::Error>;

/// Wallet-built transactions expire this many blocks after the height they
/// target (ZIP 203), so the expiry height locates the build.
const EXPIRY_DELTA: u32 = 40;

/// How far below the build height to look for the anchor in the wallet's own
/// checkpoints (cheap) and in the server's tree states (one request each).
const LOCAL_SEARCH: u32 = 2_000;
const REMOTE_SEARCH: u32 = 200;

/// What a challenge asks the prover to disclose.
pub struct DisclosureRequest {
    pub txid: TxId,
    pub msg: Vec<u8>,
    pub outputs: OutputSelection,
    pub embed_tx: bool,
}

impl Wallet {
    /// Captures (or recaptures) the kit for a transaction this wallet sent.
    ///
    /// Works for any past transaction whose spent notes the wallet knows: the
    /// paths come from the wallet's checkpoint at the anchor while it is still
    /// retained, and otherwise from the server's tree state at that height
    /// combined with the wallet's own note commitment tree.
    pub async fn capture_kit(&mut self, lwd: &mut Lightwalletd, txid: TxId) -> Result<DisclosureKit> {
        self.check_server(lwd)?;
        let chain = lwd.transaction(txid).await?;
        let (tx, mined) = match (chain, self.db.get_transaction(txid).map_err(Error::db)?) {
            (Some(c), _) => (c.tx, c.mined_height),
            (None, Some(tx)) => (tx, None),
            (None, None) => return Err(Error::TxNotFound(txid.to_string())),
        };
        let bundle = tx.ironwood_bundle().ok_or_else(|| Error::Kit(format!("transaction {txid} has no Ironwood bundle, so there is nothing to disclose")))?;
        let anchor = *bundle.anchor();
        let anchor_height = self.find_anchor_height(lwd, &tx, mined, anchor).await?;

        let fvk = self.orchard_fvk()?;
        let spent: HashSet<[u8; 32]> = bundle.actions().iter().map(|a| a.nullifier().to_bytes()).collect();
        let notes: Vec<(Note, Position)> = self
            .db
            .get_unspent_ironwood_notes_at_historical_height(self.account, anchor_height)
            .map_err(Error::db)?
            .into_iter()
            .filter(|n| spent.contains(&n.note().nullifier(&fvk).to_bytes()))
            .map(|n| (*n.note(), n.note_commitment_tree_position()))
            .collect();
        if notes.is_empty() {
            return Err(Error::Kit(format!(
                "transaction {txid} spends none of this wallet's notes; only the sender of a payment can disclose it"
            )));
        }
        let kit = self.kit_from_notes(lwd, txid, anchor, anchor_height, notes).await?;
        kit.save(&self.dir)?;
        Ok(kit)
    }

    pub fn load_kit(&self, txid: TxId) -> Result<DisclosureKit> {
        let kit = DisclosureKit::load(&self.dir, txid)?;
        if kit.network != self.network() {
            return Err(Error::Kit(format!("the kit for {txid} is for {}, this wallet is on {}", kit.network, self.network())));
        }
        Ok(kit)
    }

    /// Answers a challenge about a mined transaction this wallet sent.
    pub async fn prove(&self, lwd: &mut Lightwalletd, req: DisclosureRequest, pk: &orchard::circuit::ProvingKey) -> Result<PaymentDisclosure> {
        self.check_server(lwd)?;
        let kit = self.load_kit(req.txid)?;
        let spends = kit.witnesses()?;
        let chain = lwd.transaction(req.txid).await?.ok_or_else(|| Error::TxNotFound(req.txid.to_string()))?;
        let height = chain.mined_height.ok_or_else(|| Error::NotMined(req.txid.to_string()))?;

        let usk = self.spending_key()?;
        let sk = usk.orchard();
        let fvk = FullViewingKey::from(sk);
        let ask = SpendAuthorizingKey::from(sk);
        let request = ProveRequest {
            network: self.network(),
            tx: &chain.tx,
            height,
            msg: req.msg,
            fvk: &fvk,
            ask: &ask,
            spends,
            outputs: req.outputs,
            embed_tx: req.embed_tx,
        };
        Ok(zip311::prove_reproved(request, pk, &mut UnwrapErr(SysRng))?)
    }

    /// Builds a kit from notes and the height of the checkpoint the
    /// transaction was anchored to, checking every path against `anchor`.
    pub(crate) async fn kit_from_notes(
        &mut self,
        lwd: &mut Lightwalletd,
        txid: TxId,
        anchor: Anchor,
        anchor_height: BlockHeight,
        notes: Vec<(Note, Position)>,
    ) -> Result<DisclosureKit> {
        let positions: Vec<Position> = notes.iter().map(|(_, p)| *p).collect();
        // The wallet's own checkpoint, while it is retained. Any failure here
        // falls through to the server's tree state; every path is checked
        // against the anchor in the end either way.
        let local = self
            .db
            .with_ironwood_tree_mut(|tree| {
                positions.iter().map(|p| tree.witness_at_checkpoint_id_caching(*p, &anchor_height)).collect::<std::result::Result<Option<Vec<_>>, TreeError>>()
            })
            .ok()
            .flatten()
            .flatten();
        let paths = match local {
            Some(paths) => paths,
            None => {
                let state = lwd.tree_state(anchor_height).await?;
                let frontier = state
                    .ironwood_tree()
                    .map_err(Error::server)?
                    .to_frontier()
                    .take()
                    .ok_or_else(|| Error::Kit(format!("the Ironwood tree is empty at height {anchor_height}")))?;
                self.db.generate_ironwood_witnesses_at_historical_height(&positions, frontier, anchor_height).map_err(Error::db)?
            }
        };
        let witnesses: Vec<SpendWitness> =
            notes.into_iter().zip(paths).map(|((note, _), path)| SpendWitness { note, merkle_path: path.into() }).collect();
        DisclosureKit::new(self.network(), txid, anchor, anchor_height.into(), &witnesses)
    }

    /// Finds the block whose Ironwood tree root is the transaction's anchor.
    async fn find_anchor_height(&mut self, lwd: &mut Lightwalletd, tx: &Transaction, mined: Option<u32>, anchor: Anchor) -> Result<BlockHeight> {
        let expiry = u32::from(tx.expiry_height());
        let built_for = match (mined, expiry) {
            (Some(m), 0) => m,
            (Some(m), e) => m.min(e.saturating_sub(EXPIRY_DELTA)),
            (None, 0) => u32::from(lwd.tip().await?) + 1,
            (None, e) => e.saturating_sub(EXPIRY_DELTA),
        };
        let floor = self.birthday().saturating_sub(1);

        let low = built_for.saturating_sub(LOCAL_SEARCH).max(floor);
        let found = self
            .db
            .with_ironwood_tree_mut(|tree| {
                // A checkpoint whose root can't be computed just isn't the one.
                let found = (low..built_for)
                    .rev()
                    .find(|h| matches!(tree.root_at_checkpoint_id(&BlockHeight::from_u32(*h)), Ok(Some(r)) if Anchor::from(r) == anchor));
                Ok::<_, TreeError>(found)
            })
            .map_err(Error::db)?
            .flatten();
        if let Some(h) = found {
            return Ok(BlockHeight::from_u32(h));
        }

        let low = built_for.saturating_sub(REMOTE_SEARCH);
        for h in (low..built_for).rev() {
            let state = lwd.tree_state(BlockHeight::from_u32(h)).await?;
            if Anchor::from(state.ironwood_tree().map_err(Error::server)?.root()) == anchor {
                return Ok(BlockHeight::from_u32(h));
            }
        }
        Err(Error::Kit(format!("no block in {low}..{built_for} has this transaction's anchor as its Ironwood tree root")))
    }
}
