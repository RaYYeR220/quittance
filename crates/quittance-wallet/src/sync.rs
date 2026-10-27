//! Bringing the wallet up to the chain tip.
//!
//! The flow is the one `zcash_client_backend` documents: tell the wallet the
//! tip, scan the ranges it asks for (highest priority first) in batches of
//! compact blocks, then fetch the full transactions it wants for memos and
//! for the status of what it sent. Blocks are held in memory for one batch
//! only; nothing but the wallet database touches the disk.

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::ops::Range;

use zcash_client_backend::{
    data_api::{
        chain::{error::Error as ChainError, scan_cached_blocks, BlockSource},
        wallet::decrypt_and_store_transaction,
        TransactionDataRequest, TransactionStatus, WalletCommitmentTrees, WalletRead, WalletWrite,
    },
    proto::{compact_formats::CompactBlock, service::ShieldedProtocol},
};
use zcash_protocol::{consensus::BlockHeight, TxId};

use crate::error::{Error, Result};
use crate::lightwalletd::Lightwalletd;
use crate::wallet::Wallet;

/// Blocks downloaded and scanned per round trip.
const BATCH: u32 = 1_000;

/// How far below a detected fork the wallet rewinds before rescanning.
const REORG_MARGIN: u32 = 10;

/// Gives up rather than rewinding forever against a server that keeps
/// contradicting itself.
const MAX_REWINDS: usize = 8;

#[derive(Clone, Debug)]
pub enum SyncEvent {
    Tip(u32),
    Scanned { blocks: Range<u32>, remaining: u64 },
    Rewound { fork_at: u32, to: u32 },
    Enhanced(TxId),
    /// Unspent outputs found at the account's transparent addresses (only
    /// for a wallet that has handed one out).
    TransparentOutputs(usize),
}

#[derive(Clone, Debug)]
pub struct SyncReport {
    pub tip: u32,
    pub scanned_blocks: u64,
    pub enhanced: usize,
}

struct BatchSource(Vec<CompactBlock>);

impl BlockSource for BatchSource {
    type Error = Infallible;

    fn with_blocks<F, WalletErrT>(
        &self,
        from_height: Option<BlockHeight>,
        limit: Option<usize>,
        mut with_block: F,
    ) -> std::result::Result<(), ChainError<WalletErrT, Infallible>>
    where
        F: FnMut(CompactBlock) -> std::result::Result<(), ChainError<WalletErrT, Infallible>>,
    {
        let from = from_height.unwrap_or(BlockHeight::from_u32(0));
        for block in self.0.iter().filter(|b| b.height() >= from).take(limit.unwrap_or(usize::MAX)) {
            with_block(block.clone())?;
        }
        Ok(())
    }
}

impl Wallet {
    /// Scans from the last synced height (or the birthday) to the tip, then
    /// fetches the full transactions the wallet needs. Safe to repeat.
    pub async fn sync(&mut self, lwd: &mut Lightwalletd, mut on: impl FnMut(SyncEvent)) -> Result<SyncReport> {
        self.check_server(lwd)?;
        self.update_subtree_roots(lwd).await?;

        let tip = lwd.tip().await?;
        self.db.update_chain_tip(tip).map_err(Error::db)?;
        on(SyncEvent::Tip(tip.into()));

        // Before shielded scanning, as the backend's own sync does, so outputs
        // between the last scan and the tip are found in this pass.
        if self.watches_transparent() {
            let found = self.refresh_utxos(lwd).await?;
            on(SyncEvent::TransparentOutputs(found));
        }

        let mut scanned_blocks = 0u64;
        let mut rewinds = 0;
        loop {
            let ranges = self.db.suggest_scan_ranges().map_err(Error::db)?;
            let Some(range) = ranges.first() else { break };
            let remaining: u64 = ranges.iter().map(|r| r.len() as u64).sum();
            let batch = range.split_at(range.block_range().start + BATCH).map_or_else(|| range.clone(), |(head, _)| head);
            let blocks = batch.block_range().clone();

            let compact = lwd.blocks(blocks.clone()).await?;
            let prior = lwd.tree_state(blocks.start - 1).await?.to_chain_state().map_err(Error::server)?;
            match scan_cached_blocks(&self.params(), &BatchSource(compact), &mut self.db, blocks.start, &prior, batch.len()) {
                Ok(_) => {
                    scanned_blocks += batch.len() as u64;
                    on(SyncEvent::Scanned {
                        blocks: u32::from(blocks.start)..u32::from(blocks.end),
                        remaining: remaining - batch.len() as u64,
                    });
                }
                Err(ChainError::Scan(e)) if e.is_continuity_error() => {
                    rewinds += 1;
                    if rewinds > MAX_REWINDS {
                        return Err(Error::Server(format!("the chain keeps reorganizing under the wallet ({e}); try again later or another server")));
                    }
                    let fork_at = e.at_height();
                    let to = self.db.truncate_to_height(fork_at.saturating_sub(REORG_MARGIN)).map_err(|e| {
                        Error::Database(format!("cannot rewind past a reorg at {fork_at} ({e}); re-create the wallet to rescan from its birthday"))
                    })?;
                    // Truncation trims the scan queue; re-announcing the tip queues the replacement blocks.
                    self.db.update_chain_tip(tip).map_err(Error::db)?;
                    on(SyncEvent::Rewound { fork_at: fork_at.into(), to: to.into() });
                }
                Err(e) => return Err(Error::Database(format!("scanning {}..{}: {e}", blocks.start, blocks.end))),
            }
        }

        let enhanced = self.enhance(lwd, tip, &mut on).await?;
        Ok(SyncReport { tip: tip.into(), scanned_blocks, enhanced })
    }

    pub(crate) fn check_server(&self, lwd: &Lightwalletd) -> Result<()> {
        if lwd.network() != self.network() {
            return Err(Error::Server(format!("{} serves {}, but this wallet is on {}", lwd.url(), lwd.network(), self.network())));
        }
        Ok(())
    }

    /// Completed subtree roots let the wallet witness its notes without
    /// scanning the whole history of each pool.
    async fn update_subtree_roots(&mut self, lwd: &mut Lightwalletd) -> Result<()> {
        let sapling = lwd.sapling_subtree_roots().await?;
        self.db.put_sapling_subtree_roots(0, &sapling).map_err(Error::db)?;
        let orchard = lwd.subtree_roots(ShieldedProtocol::Orchard).await?;
        self.db.put_orchard_subtree_roots(0, &orchard).map_err(Error::db)?;
        let ironwood = lwd.subtree_roots(ShieldedProtocol::Ironwood).await?;
        self.db.put_ironwood_subtree_roots(0, &ironwood).map_err(Error::db)?;
        Ok(())
    }

    /// Answers the wallet's requests for full transactions: memos and sent
    /// outputs of transactions it found, and the mined status of its own.
    async fn enhance(&mut self, lwd: &mut Lightwalletd, tip: BlockHeight, on: &mut impl FnMut(SyncEvent)) -> Result<usize> {
        let mut answered = BTreeSet::new();
        let mut enhanced = 0;
        loop {
            let requests: Vec<_> = self.db.transaction_data_requests().map_err(Error::db)?.into_iter().filter(|r| !answered.contains(r)).collect();
            if requests.is_empty() {
                return Ok(enhanced);
            }
            for request in requests {
                match &request {
                    TransactionDataRequest::GetStatus(txid) => {
                        let status = match lwd.transaction(*txid).await? {
                            None => TransactionStatus::TxidNotRecognized,
                            Some(c) => c.mined_height.map_or(TransactionStatus::NotInMainChain, |h| TransactionStatus::Mined(h.into())),
                        };
                        self.db.set_transaction_status(*txid, status).map_err(Error::db)?;
                    }
                    TransactionDataRequest::Enhancement(txid) => match lwd.transaction(*txid).await? {
                        None => self.db.set_transaction_status(*txid, TransactionStatus::TxidNotRecognized).map_err(Error::db)?,
                        Some(c) => {
                            decrypt_and_store_transaction(&self.params(), &mut self.db, &c.tx, c.mined_height.map(BlockHeight::from))
                                .map_err(Error::db)?;
                            enhanced += 1;
                            on(SyncEvent::Enhanced(*txid));
                        }
                    },
                    // A wallet that never handed out a transparent address
                    // doesn't reveal its transparent addresses to the server.
                    TransactionDataRequest::TransactionsInvolvingAddress(r) => {
                        if self.watches_transparent() {
                            enhanced += self.check_transparent_address(lwd, r.clone(), tip).await?;
                        }
                    }
                }
                answered.insert(request);
            }
        }
    }
}
