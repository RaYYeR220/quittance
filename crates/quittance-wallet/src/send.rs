//! Ironwood-only payments.
//!
//! The pool rules are enforced twice: the recipient is reduced to its Orchard
//! receiver before anything is proposed, and the proposal the wallet comes
//! back with is checked to spend, pay and return change in Ironwood alone. A
//! proposal that would touch any other pool is refused, never adjusted.

use std::convert::Infallible;
use std::path::PathBuf;
use std::str::FromStr;

use rand::{rand_core::UnwrapErr, rngs::SysRng};
use zcash_client_backend::{
    data_api::{
        error::Error as DataError,
        wallet::{
            create_proposed_transactions,
            input_selection::{GreedyInputSelector, LockedInputPolicy, SpendPolicy},
            propose_send_max_transfer, propose_transfer, ConfirmationsPolicy, SpendingKeys,
        },
        MaxSpendMode, WalletRead,
    },
    fees::{standard::SingleOutputChangeStrategy, DustOutputPolicy, StandardFeeRule},
    proposal::{Proposal, Step},
    wallet::OvkPolicy,
};
use zcash_client_sqlite::{util::SystemClock, ReceivedNoteId};
use zcash_proofs::prover::LocalTxProver;
use zcash_protocol::{
    memo::{Memo, MemoBytes},
    value::Zatoshis,
    PoolType, ShieldedPool, TxId,
};
use zip321::{Payment, TransactionRequest};

use crate::error::{Error, Result};
use crate::kit::DisclosureKit;
use crate::lightwalletd::Lightwalletd;
use crate::recipient::IronwoodRecipient;
use crate::wallet::Wallet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Amount {
    Zat(u64),
    /// Everything spendable in Ironwood, less the fee.
    Max,
}

#[derive(Clone, Debug)]
pub struct PaymentRequest {
    /// A unified address with an Orchard receiver.
    pub to: String,
    pub amount: Amount,
    pub memo: Option<String>,
}

pub struct Sent {
    pub txid: TxId,
    pub paid_zat: u64,
    pub fee_zat: u64,
    /// The address actually paid: the Orchard receiver of the requested one, on its own.
    pub paid_to: String,
    /// Where the disclosure kit was saved, or why it could not be.
    pub kit: std::result::Result<PathBuf, Error>,
}

impl Wallet {
    /// Builds, records and broadcasts an Ironwood payment, capturing its
    /// disclosure kit before it leaves the wallet.
    pub async fn send(&mut self, lwd: &mut Lightwalletd, req: &PaymentRequest) -> Result<Sent> {
        self.check_server(lwd)?;
        let network = self.network();
        let params = self.params();
        let recipient = IronwoodRecipient::parse(&req.to, network)?;
        let memo = req
            .memo
            .as_deref()
            .map(|m| Memo::from_str(m).map(MemoBytes::from).map_err(|_| Error::Policy("the memo is longer than 512 bytes".into())))
            .transpose()?;

        // Fail on a wrong or missing mnemonic before doing any work.
        let usk = self.spending_key()?;

        let proposal = match req.amount {
            Amount::Zat(0) => return Err(Error::Policy("the amount must be more than zero".into())),
            Amount::Zat(zat) => {
                let value = Zatoshis::from_u64(zat).map_err(|_| Error::Policy(format!("{zat} zatoshis is more than can exist")))?;
                let payment = Payment::new(recipient.to_address(network), Some(value), memo, None, None, vec![])
                    .map_err(|e| Error::Policy(format!("{e:?}")))?;
                let request = TransactionRequest::new(vec![payment]).map_err(|e| Error::Policy(e.to_string()))?;
                let change = SingleOutputChangeStrategy::new(StandardFeeRule::Zip317, None, ShieldedPool::Ironwood, DustOutputPolicy::default());
                propose_transfer::<_, _, _, _, Infallible>(
                    &mut self.db,
                    &params,
                    self.account,
                    &GreedyInputSelector::new(),
                    &change,
                    request,
                    ConfirmationsPolicy::default(),
                    &SpendPolicy::shielded_pools([ShieldedPool::Ironwood]),
                    None,
                    None,
                )
                .map_err(|e| propose_error(e, network))?
            }
            Amount::Max => propose_send_max_transfer::<_, _, _, Infallible>(
                &mut self.db,
                &params,
                self.account,
                &[ShieldedPool::Ironwood],
                &StandardFeeRule::Zip317,
                recipient.to_address(network),
                memo,
                MaxSpendMode::MaxSpendable,
                ConfirmationsPolicy::default(),
                &LockedInputPolicy::Exclude,
                None,
            )
            .map_err(|e| propose_error(e, network))?,
        };
        let step = ironwood_only(&proposal)?;
        let fee_zat = step.balance().fee_required().into_u64();
        let paid_zat = step.transaction_request().payments().values().filter_map(|p| p.amount()).map(|z| z.into_u64()).sum();

        let prover = LocalTxProver::bundled();
        let txids = create_proposed_transactions::<_, _, Infallible, _, Infallible, _>(
            &mut self.db,
            &params,
            &SystemClock,
            &mut UnwrapErr(SysRng),
            &prover,
            &prover,
            &SpendingKeys::from_unified_spending_key(usk),
            OvkPolicy::Sender,
            &proposal,
            None,
        )
        .map_err(|e| Error::Build(e.to_string()))?;
        let txid = *txids.first();
        let tx = self.db.get_transaction(txid).map_err(Error::db)?.ok_or_else(|| Error::Build(format!("transaction {txid} was built but not stored")))?;

        // The spent notes and the anchor checkpoint are certainly at hand now;
        // the kit is saved before broadcasting so a crash can't lose it.
        let kit = self.kit_for_new_tx(lwd, &tx, step).await.and_then(|k| k.save(&self.dir));

        lwd.broadcast(&tx).await?;
        Ok(Sent { txid, paid_zat, fee_zat, paid_to: recipient.encode(network), kit })
    }

    async fn kit_for_new_tx(
        &mut self,
        lwd: &mut Lightwalletd,
        tx: &zcash_primitives::transaction::Transaction,
        step: &Step<ReceivedNoteId>,
    ) -> Result<DisclosureKit> {
        let bundle = tx.ironwood_bundle().ok_or_else(|| Error::Kit("the transaction has no Ironwood bundle".into()))?;
        let anchor_height = step.anchor_height().ok_or_else(|| Error::Kit("the proposal has no anchor height".into()))?;
        let notes = step
            .shielded_inputs()
            .into_iter()
            .flat_map(|s| s.notes().iter())
            .filter_map(|n| match n.note() {
                zcash_client_backend::wallet::Note::Orchard { note, pool: orchard::ValuePool::Ironwood } => {
                    Some((*note, n.note_commitment_tree_position()))
                }
                _ => None,
            })
            .collect();
        self.kit_from_notes(lwd, tx.txid(), *bundle.anchor(), anchor_height, notes).await
    }
}

fn propose_error<DE, TE, SE, FE, CE, N>(e: DataError<DE, TE, SE, FE, CE, N>, network: zip311::Network) -> Error
where
    DataError<DE, TE, SE, FE, CE, N>: std::fmt::Display,
{
    match e {
        DataError::InsufficientFunds { available, required } => Error::Propose(format!(
            "not enough spendable Ironwood funds: {} spendable, {} needed with the fee. Funds received from others need 10 confirmations (3 for your own change); see `quittance wallet balance`",
            amount(available.into_u64(), network),
            amount(required.into_u64(), network)
        )),
        DataError::ScanRequired => Error::NotSynced,
        e => Error::Propose(format!("cannot propose the payment: {e}")),
    }
}

/// Accepts a proposal only if its single transaction spends, pays and
/// returns change in Ironwood alone.
fn ironwood_only<F, N>(proposal: &Proposal<F, N>) -> Result<&Step<N>> {
    let steps = proposal.steps();
    if steps.len() != 1 {
        return Err(Error::Policy(format!("the payment would take {} transactions; only single-transaction payments are made", steps.len())));
    }
    let step = steps.first();
    for pool in [PoolType::Transparent, PoolType::SAPLING, PoolType::ORCHARD] {
        if step.involves(pool) {
            return Err(Error::Policy(format!("the proposed transaction involves the {pool} pool; this wallet spends and pays only Ironwood")));
        }
    }
    if !step.input_in_pool(PoolType::IRONWOOD) {
        return Err(Error::Policy("the proposed transaction spends no Ironwood note".into()));
    }
    if let Some(pool) = step.payment_pools().values().find(|p| **p != PoolType::IRONWOOD) {
        return Err(Error::Policy(format!("the payment would be made in the {pool} pool, not Ironwood")));
    }
    Ok(step)
}

/// An amount with the network's ticker: ZEC on mainnet, TAZ on testnet.
fn amount(zat: u64, network: zip311::Network) -> String {
    let ticker = match network {
        zip311::Network::Main => "ZEC",
        zip311::Network::Test => "TAZ",
    };
    format!("{} {ticker}", zec(zat))
}

fn zec(zat: u64) -> String {
    let s = format!("{}.{:08}", zat / 100_000_000, zat % 100_000_000);
    let s = s.trim_end_matches('0');
    if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{amount, zec};

    #[test]
    fn zec_amounts() {
        assert_eq!(amount(1_010_000, zip311::Network::Test), "0.0101 TAZ");
        assert_eq!(zec(0), "0.0");
        assert_eq!(zec(1_000_000), "0.01");
        assert_eq!(zec(12_500_000), "0.125");
        assert_eq!(zec(100_000_000), "1.0");
        assert_eq!(zec(10_000), "0.0001");
    }
}
