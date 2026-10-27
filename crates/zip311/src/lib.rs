//! ZIP 311 payment disclosures for the Zcash Ironwood pool.
//!
//! A payment disclosure lets the sender of a shielded transaction prove, to one
//! verifier and bound to that verifier's challenge, that they paid a given
//! address a given amount with a given memo, without handing over a viewing key
//! and without revealing anything else about their wallet.
//!
//! [ZIP 311] specifies disclosures for Sapling and leaves Orchard as future
//! work. This crate implements the Ironwood case (the pool that replaced Orchard
//! after the 2026 soundness fix) using the same construction ZIP 311 uses for
//! Sapling spends:
//!
//! * **Spend authority.** The prover re-proves the spend of the note that the
//!   transaction consumed, under the transaction's own anchor, with fresh
//!   randomness. The re-proof is itself an Ironwood bundle with outputs
//!   disabled; it can never be mined because its nullifier is already spent.
//!   The disclosure digest takes the place of the sighash, so the spend
//!   authorization signature binds the disclosure to the spend key. Nothing new
//!   is trusted: the circuit, the signature scheme and the verifying key are the
//!   ones consensus already uses.
//! * **Outputs.** For each disclosed output the prover reveals its outgoing
//!   cipher key (`ock`), which opens that output and no other.
//!
//! The bundle's value balance and binding signature are never published, so a
//! disclosure does not reveal the value of the notes that were spent.
//!
//! [ZIP 311]: https://zips.z.cash/zip-0311

mod challenge;
mod encoding;
mod error;
mod format;
mod network;
mod prove;
mod verify;

pub use challenge::Challenge;
pub use error::{DecodeError, ProveError};
pub use format::{OutputDisclosure, PaymentDisclosure, ReprovedAction, ReprovedSpends, ResignedSpend};
pub use network::Network;
pub use prove::{prove_bundle, prove_reproved, OutputSelection, ProveRequest, SpendWitness};
pub use verify::{
    verify, verify_bundle, Check, CheckStatus, MemoView, Payment, Revealed, SpendInfo, Verdict, VerifyContext,
    NOT_REVEALED,
};

/// The circuit version every Ironwood proof (including disclosure re-proofs) uses.
pub const CIRCUIT_VERSION: orchard::circuit::OrchardCircuitVersion = orchard::circuit::OrchardCircuitVersion::PostNu6_3;

/// Builds the proving key for disclosure re-proofs. Takes a few seconds; build once and reuse.
pub fn proving_key() -> orchard::circuit::ProvingKey {
    orchard::circuit::ProvingKey::build(CIRCUIT_VERSION)
}

/// Builds the verifying key for disclosure re-proofs. Build once and reuse.
pub fn verifying_key() -> orchard::circuit::VerifyingKey {
    orchard::circuit::VerifyingKey::build(CIRCUIT_VERSION)
}
