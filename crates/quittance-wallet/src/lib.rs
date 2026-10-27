//! An Ironwood-only light wallet that keeps what a payment disclosure needs.
//!
//! A ZIP 311 disclosure re-proves the spend of the notes a transaction
//! consumed, under that transaction's anchor. That takes the spent notes and
//! their Merkle paths at the anchor, which a wallet holds while it builds the
//! transaction and normally forgets soon after. This wallet captures them at
//! send time into a *disclosure kit* (see [`DisclosureKit`]) stored next to
//! the wallet, so the sender can answer a verifier's challenge whenever it
//! comes. Kits can also be captured afterwards, for any transaction the wallet
//! sent.
//!
//! The wallet is deliberately narrow. Payments spend only Ironwood notes and
//! pay only Orchard receivers (which is where Ironwood funds go). For funding
//! from sources that can only pay a transparent address, the wallet hands one
//! out on request and can shield what arrives there into Ironwood; it never
//! pays a transparent address and never spends transparent funds any other
//! way. Sapling and Orchard balances are shown but never spent. It talks to
//! one lightwalletd server over TLS and keeps no keys on disk: the database
//! holds the viewing key, and spending or proving reads the mnemonic file
//! again.

mod config;
mod disclose;
mod error;
mod history;
mod kit;
mod lightwalletd;
mod mnemonic;
mod recipient;
mod send;
mod sync;
mod transparent;
mod wallet;

pub use config::WalletConfig;
pub use disclose::DisclosureRequest;
pub use error::{Error, Result};
pub use history::TxSummary;
pub use kit::{parse_txid, DisclosureKit, KitNote};
pub use lightwalletd::{default_servers, ChainTx, Lightwalletd};
pub use recipient::{IronwoodRecipient, RecipientError};
pub use send::{Amount, PaymentRequest, Sent};
pub use sync::{SyncEvent, SyncReport};
pub use transparent::Shielded;
pub use wallet::{BalanceReport, PoolBalance, Wallet};

pub use zcash_primitives::transaction::Transaction;
pub use zcash_protocol::TxId;

/// Parses a raw transaction mined (or to be mined) at `height`.
pub fn parse_transaction(raw: &[u8], network: zip311::Network, height: u32) -> std::io::Result<Transaction> {
    use zcash_protocol::consensus::{BlockHeight, BranchId};
    Transaction::read(raw, BranchId::for_height(&network.params(), BlockHeight::from_u32(height)))
}
