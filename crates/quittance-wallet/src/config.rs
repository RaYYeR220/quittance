use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip311::Network;

use crate::error::{Error, Result};

const CONFIG_FILE: &str = "wallet.json";
const DB_FILE: &str = "wallet.sqlite";
const KITS_DIR: &str = "kits";

/// What a wallet directory records besides its database.
///
/// Keys are deliberately not stored: the database holds only the viewing key,
/// and anything that spends or signs reads the mnemonic file again. A wallet
/// directory alone can watch the account but never move or disclose funds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletConfig {
    pub network: Network,
    pub birthday: u32,
    /// Absolute path of the file holding the account's mnemonic.
    pub mnemonic_file: PathBuf,
}

impl WalletConfig {
    pub(crate) fn read(dir: &Path) -> Result<Self> {
        let path = config_path(dir);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Error::NoWallet(dir.to_owned())),
            Err(e) => return Err(Error::io(&path, e)),
        };
        serde_json::from_str(&text).map_err(|e| Error::Config { path, reason: e.to_string() })
    }

    pub(crate) fn write(&self, dir: &Path) -> Result<()> {
        let path = config_path(dir);
        let text = serde_json::to_string_pretty(self).expect("config serializes");
        std::fs::write(&path, text + "\n").map_err(|e| Error::io(&path, e))
    }
}

pub(crate) fn config_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE)
}

pub(crate) fn db_path(dir: &Path) -> PathBuf {
    dir.join(DB_FILE)
}

pub(crate) fn kits_dir(dir: &Path) -> PathBuf {
    dir.join(KITS_DIR)
}
