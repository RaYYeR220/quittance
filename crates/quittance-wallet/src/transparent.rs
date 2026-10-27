//! Transparent funds, received only to be shielded.
//!
//! Some sources, such as exchanges and bridges, can only pay a transparent
//! address. The wallet hands one out only when asked, and from then on sync
//! watches the account's own transparent addresses. The one thing the wallet
//! ever does with transparent funds is move them into Ironwood with
//! [`Wallet::shield`]; it never pays a transparent address and never spends
//! transparent funds any other way.

use std::convert::Infallible;
use std::num::NonZeroU32;

use rand::{rand_core::UnwrapErr, rngs::SysRng};
use transparent::{
    address::Script,
    bundle::{OutPoint, TxOut},
    keys::IncomingViewingKey,
};
use zcash_client_backend::{
    data_api::{
        wallet::{
            create_proposed_transactions, decrypt_and_store_transaction, input_selection::GreedyInputSelector,
            propose_shielding, ConfirmationsPolicy, SpendingKeys, TargetHeight,
        },
        CoinbaseFilter, TransactionsInvolvingAddress, WalletRead, WalletWrite,
    },
    fees::{standard::SingleOutputChangeStrategy, DustOutputPolicy, StandardFeeRule},
    wallet::{OvkPolicy, WalletTransparentOutput},
};
use zcash_client_sqlite::util::SystemClock;
use zcash_keys::encoding::AddressCodec;
use zcash_proofs::prover::LocalTxProver;
use zcash_protocol::{consensus::BlockHeight, value::Zatoshis, PoolType, ShieldedPool, TxId};

use crate::error::{Error, Result};
use crate::lightwalletd::Lightwalletd;
use crate::wallet::Wallet;

/// Shielding spends transparent outputs with one confirmation.
fn shielding_policy() -> ConfirmationsPolicy {
    ConfirmationsPolicy::new_symmetrical(NonZeroU32::MIN, false)
}

pub struct Shielded {
    pub txid: TxId,
    /// Value that arrives in Ironwood, after the fee.
    pub shielded_zat: u64,
    pub fee_zat: u64,
}

impl Wallet {
    /// The account's first external transparent (P2PKH) address.
    ///
    /// Handing one out turns on transparent watching for this wallet, so the
    /// next sync finds what is paid to it.
    pub fn transparent_address(&mut self) -> Result<String> {
        let ufvk = self.ufvk()?;
        let key = ufvk.p2pkh().ok_or_else(|| Error::Database("the account has no transparent key".into()))?;
        let ivk = key.derive_external_ivk().map_err(|e| Error::Database(format!("cannot derive a transparent address: {e}")))?;
        let (address, _) = ivk.default_address();
        if !self.config.transparent {
            self.config.transparent = true;
            self.config.write(&self.dir)?;
        }
        Ok(address.encode(&self.params()))
    }

    pub fn watches_transparent(&self) -> bool {
        self.config.transparent
    }

    /// Records the unspent outputs paid to the account's transparent addresses.
    pub(crate) async fn refresh_utxos(&mut self, lwd: &mut Lightwalletd) -> Result<usize> {
        let params = self.params();
        let addresses: Vec<String> =
            self.db.get_transparent_receivers(self.account, true, true).map_err(Error::db)?.into_keys().map(|a| a.encode(&params)).collect();
        if addresses.is_empty() {
            return Ok(0);
        }
        let start = self.db.utxo_query_height(self.account).map_err(Error::db)?;
        let replies = lwd.address_utxos(addresses, start).await?;
        let found = replies.len();
        for r in replies {
            let misbehaving = || Error::Server(format!("malformed unspent output from the server for {}", r.address));
            let txid: [u8; 32] = r.txid.as_slice().try_into().map_err(|_| misbehaving())?;
            let index = u32::try_from(r.index).map_err(|_| misbehaving())?;
            let value = Zatoshis::from_nonnegative_i64(r.value_zat).map_err(|_| misbehaving())?;
            let height = u32::try_from(r.height).map_err(|_| misbehaving())?;
            let output = WalletTransparentOutput::from_parts(
                OutPoint::new(txid, index),
                TxOut::new(value, Script(zcash_script::script::Code(r.script.clone()))),
                Some(BlockHeight::from_u32(height)),
                Some(self.account),
                None,
                None,
            )
            .ok_or_else(misbehaving)?;
            self.db.put_received_transparent_utxo(&output).map_err(Error::db)?;
        }
        Ok(found)
    }

    /// Answers the wallet's request for the transactions touching one of its
    /// transparent addresses within a block range.
    pub(crate) async fn check_transparent_address(&mut self, lwd: &mut Lightwalletd, request: TransactionsInvolvingAddress, tip: BlockHeight) -> Result<usize> {
        let end = request.block_range_end().unwrap_or(tip + 1);
        if end <= request.block_range_start() {
            return Ok(0);
        }
        let txs = lwd.address_transactions(request.address().encode(&self.params()), request.block_range_start()..end).await?;
        if txs.is_empty() {
            self.db.notify_address_checked(request, end - 1).map_err(Error::db)?;
        }
        for c in &txs {
            decrypt_and_store_transaction(&self.params(), &mut self.db, &c.tx, c.mined_height.map(BlockHeight::from)).map_err(Error::db)?;
        }
        Ok(txs.len())
    }

    /// Moves every confirmed transparent output of the account into Ironwood,
    /// to the account's own internal address, and broadcasts the transaction.
    pub async fn shield(&mut self, lwd: &mut Lightwalletd) -> Result<Shielded> {
        self.check_server(lwd)?;
        let params = self.params();
        let tip = self.db.chain_height().map_err(Error::db)?.ok_or(Error::NotSynced)?;
        let policy = shielding_policy();
        let balances = self.db.get_transparent_balances(self.account, TargetHeight::from(tip + 1), policy).map_err(Error::db)?;
        let from: Vec<_> = balances.iter().filter(|(_, (_, b))| b.spendable_value() > Zatoshis::ZERO).map(|(a, _)| *a).collect();
        if from.is_empty() {
            return Err(Error::NothingToShield);
        }
        let usk = self.spending_key()?;

        let change = SingleOutputChangeStrategy::new(StandardFeeRule::Zip317, None, ShieldedPool::Ironwood, DustOutputPolicy::default());
        let proposal = propose_shielding::<_, _, _, _, Infallible>(
            &mut self.db,
            &params,
            &GreedyInputSelector::new(),
            &change,
            Zatoshis::ZERO,
            &from,
            self.account,
            policy,
            CoinbaseFilter::AllTransparentOutputs,
            None,
        )
        .map_err(|e| Error::Propose(format!("cannot propose shielding: {e}")))?;

        let steps = proposal.steps();
        let step = steps.first();
        let into_ironwood_only = steps.len() == 1
            && step.is_shielding()
            && step.payment_pools().is_empty()
            && !step.involves(PoolType::SAPLING)
            && !step.involves(PoolType::ORCHARD)
            && !step.balance().proposed_change().is_empty()
            && step.balance().proposed_change().iter().all(|c| c.output_pool() == PoolType::IRONWOOD);
        if !into_ironwood_only {
            return Err(Error::Policy("the proposed shielding transaction would not move the funds into Ironwood alone".into()));
        }
        let shielded_zat = step.balance().proposed_change().iter().map(|c| c.value().into_u64()).sum();
        let fee_zat = step.balance().fee_required().into_u64();

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
        lwd.broadcast(&tx).await?;
        Ok(Shielded { txid, shielded_zat, fee_zat })
    }
}

