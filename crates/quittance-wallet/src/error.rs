use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::recipient::RecipientError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no wallet in {0}: create one with `quittance wallet init`")]
    NoWallet(PathBuf),
    #[error("{0} already holds a wallet; pick an empty directory")]
    WalletExists(PathBuf),
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("wallet config {path} is unreadable: {reason}")]
    Config { path: PathBuf, reason: String },
    /// Never carries any part of the phrase.
    #[error("mnemonic: {0}")]
    Mnemonic(String),
    #[error("the mnemonic in {0} does not belong to this wallet")]
    WrongMnemonic(PathBuf),
    #[error("wallet database: {0}")]
    Database(String),
    #[error("lightwalletd: {0}")]
    Server(String),
    #[error("the wallet has not synced yet: run `quittance wallet sync`")]
    NotSynced,
    #[error(transparent)]
    Recipient(#[from] RecipientError),
    #[error("refused: {0}")]
    Policy(String),
    #[error("{0}")]
    Propose(String),
    #[error("could not build the transaction: {0}")]
    Build(String),
    #[error("lightwalletd rejected the transaction (code {code}): {message}")]
    Broadcast { code: i32, message: String },
    #[error("transaction {0} is not known to lightwalletd")]
    TxNotFound(String),
    #[error("transaction {0} is not mined yet; a disclosure names the block it was mined in, so wait for one confirmation")]
    NotMined(String),
    #[error("disclosure kit: {0}")]
    Kit(String),
    #[error("no disclosure kit for {txid}: run `quittance kit capture --txid {txid}` first")]
    NoKit { txid: String },
    #[error(transparent)]
    Prove(#[from] zip311::ProveError),
}

impl Error {
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Error::Io { path: path.to_owned(), source }
    }

    pub(crate) fn db(e: impl std::fmt::Display) -> Self {
        Error::Database(e.to_string())
    }

    pub(crate) fn server(e: impl std::fmt::Display) -> Self {
        Error::Server(e.to_string())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
