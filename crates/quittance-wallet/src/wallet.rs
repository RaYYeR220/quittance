use std::path::{Path, PathBuf};

use rand::{rand_core::UnwrapErr, rngs::SysRng};
use secrecy::{ExposeSecret, SecretVec};
use serde::Serialize;
use zcash_client_backend::data_api::{Account, AccountBirthday, Balance, WalletRead, WalletWrite};
use zcash_client_backend::data_api::wallet::ConfirmationsPolicy;
use zcash_client_sqlite::{util::SystemClock, wallet::init::init_wallet_db, AccountUuid, WalletDb};
use zcash_keys::keys::{UnifiedAddressRequest, UnifiedFullViewingKey, UnifiedSpendingKey};
use zcash_protocol::consensus::{self, BlockHeight};
use zip311::Network;

use crate::config::{config_path, db_path, WalletConfig};
use crate::error::{Error, Result};
use crate::lightwalletd::Lightwalletd;
use crate::mnemonic;

pub(crate) type Db = WalletDb<rusqlite::Connection, consensus::Network, SystemClock, UnwrapErr<SysRng>>;

/// The account name recorded in the database; the wallet has exactly one account.
const ACCOUNT_NAME: &str = "quittance";

/// A single-account wallet in a directory: `wallet.json`, `wallet.sqlite` and
/// the `kits/` it captures.
pub struct Wallet {
    pub(crate) dir: PathBuf,
    pub(crate) config: WalletConfig,
    pub(crate) db: Db,
    pub(crate) account: AccountUuid,
}

/// Balances in zatoshis.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct PoolBalance {
    pub spendable: u64,
    /// Received or change value still waiting for confirmations.
    pub pending: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BalanceReport {
    pub network: Network,
    pub chain_tip: u32,
    pub scanned_to: u32,
    /// The only pool this wallet spends from.
    pub ironwood: PoolBalance,
    /// Shown so nothing is hidden, but never spent by this wallet.
    pub orchard: PoolBalance,
    pub sapling: PoolBalance,
    /// Funds at the account's transparent addresses, waiting to be shielded.
    pub transparent: PoolBalance,
    /// Whether sync looks for transparent funds at all (see [`Wallet::transparent_address`]).
    pub watches_transparent: bool,
}

impl Wallet {
    /// Creates a wallet in `dir` for the account-0 keys of the mnemonic in
    /// `mnemonic_file`, scanning from `birthday` on.
    pub async fn init(dir: &Path, mnemonic_file: &Path, birthday: u32, lwd: &mut Lightwalletd) -> Result<Wallet> {
        let network = lwd.network();
        if config_path(dir).exists() || db_path(dir).exists() {
            return Err(Error::WalletExists(dir.to_owned()));
        }
        let mnemonic_file = std::path::absolute(mnemonic_file).map_err(|e| Error::io(mnemonic_file, e))?;
        let seed = mnemonic::read_seed(&mnemonic_file)?;

        let tip = u32::from(lwd.tip().await?);
        if birthday == 0 || birthday > tip {
            return Err(Error::Birthday { birthday, tip });
        }
        // The account's starting point is the tree state at the end of the block before its birthday.
        let state = lwd.tree_state(BlockHeight::from_u32(birthday - 1)).await?;
        let start = AccountBirthday::from_treestate(state, None).map_err(|e| Error::Server(format!("unusable tree state: {e:?}")))?;

        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let mut db = open_db(dir, network)?;
        init_wallet_db(&mut db, None).map_err(Error::db)?;
        let (account, _) = db.create_account(ACCOUNT_NAME, &seed, &start, None).map_err(Error::db)?;

        let config = WalletConfig { network, birthday, mnemonic_file, transparent: false };
        config.write(dir)?;
        Ok(Wallet { dir: dir.to_owned(), config, db, account })
    }

    pub fn open(dir: &Path) -> Result<Wallet> {
        let config = WalletConfig::read(dir)?;
        if !db_path(dir).exists() {
            return Err(Error::NoWallet(dir.to_owned()));
        }
        let db = open_db(dir, config.network)?;
        let account = *db
            .get_account_ids()
            .map_err(Error::db)?
            .first()
            .ok_or_else(|| Error::Database("the wallet database has no account".into()))?;
        Ok(Wallet { dir: dir.to_owned(), config, db, account })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn network(&self) -> Network {
        self.config.network
    }

    pub fn birthday(&self) -> u32 {
        self.config.birthday
    }

    /// Where the disclosure kit for `txid` is (or would be) kept.
    pub fn kit_path(&self, txid: zcash_protocol::TxId) -> PathBuf {
        crate::kit::kit_path(&self.dir, &txid.to_string())
    }

    pub(crate) fn params(&self) -> consensus::Network {
        self.config.network.params()
    }

    pub(crate) fn ufvk(&self) -> Result<UnifiedFullViewingKey> {
        self.db
            .get_account(self.account)
            .map_err(Error::db)?
            .and_then(|a| a.ufvk().cloned())
            .ok_or_else(|| Error::Database("the account has no viewing key".into()))
    }

    pub(crate) fn orchard_fvk(&self) -> Result<orchard::keys::FullViewingKey> {
        self.ufvk()?.orchard().cloned().ok_or_else(|| Error::Database("the account has no Orchard viewing key".into()))
    }

    /// Reads the seed and checks it belongs to this wallet's account.
    fn seed(&self) -> Result<SecretVec<u8>> {
        let seed = mnemonic::read_seed(&self.config.mnemonic_file)?;
        if !self.db.validate_seed(self.account, &seed).map_err(Error::db)? {
            return Err(Error::WrongMnemonic(self.config.mnemonic_file.clone()));
        }
        Ok(seed)
    }

    pub(crate) fn spending_key(&self) -> Result<UnifiedSpendingKey> {
        let seed = self.seed()?;
        UnifiedSpendingKey::from_seed(&self.params(), seed.expose_secret(), zip32::AccountId::ZERO)
            .map_err(|e| Error::Mnemonic(format!("cannot derive the account's keys: {e:?}")))
    }

    /// The account's address with an Orchard receiver only: payments to it land in Ironwood.
    pub fn address(&self) -> Result<String> {
        let (ua, _) = self
            .ufvk()?
            .default_address(UnifiedAddressRequest::ORCHARD)
            .map_err(|e| Error::Database(format!("cannot derive an address: {e:?}")))?;
        Ok(ua.encode(&self.params()))
    }

    /// Balances under the standard confirmation policy (3 confirmations for
    /// the wallet's own change, 10 for funds received from others), or `None`
    /// before the first sync.
    pub fn balance(&self) -> Result<Option<BalanceReport>> {
        let Some(summary) = self.db.get_wallet_summary(ConfirmationsPolicy::default()).map_err(Error::db)? else {
            return Ok(None);
        };
        let Some(b) = summary.account_balances().get(&self.account) else {
            return Ok(None);
        };
        Ok(Some(BalanceReport {
            network: self.network(),
            chain_tip: summary.chain_tip_height().into(),
            scanned_to: summary.fully_scanned_height().into(),
            ironwood: pool(b.ironwood_balance()),
            orchard: pool(b.orchard_balance()),
            sapling: pool(b.sapling_balance()),
            transparent: pool(&b.unshielded_balance()),
            watches_transparent: self.config.transparent,
        }))
    }
}

fn pool(b: &Balance) -> PoolBalance {
    PoolBalance {
        spendable: b.spendable_value().into_u64(),
        pending: (b.change_pending_confirmation() + b.value_pending_spendability()).map_or(u64::MAX, |z| z.into_u64()),
        total: b.total().into_u64(),
    }
}

fn open_db(dir: &Path, network: Network) -> Result<Db> {
    WalletDb::for_path(db_path(dir), network.params(), SystemClock, UnwrapErr(SysRng)).map_err(Error::db)
}
