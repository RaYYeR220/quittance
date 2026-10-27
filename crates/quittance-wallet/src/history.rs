//! The wallet's recent transactions, read from the views `zcash_client_sqlite`
//! maintains for exactly this purpose.

use rusqlite::{named_params, Connection, OpenFlags};
use serde::Serialize;
use zcash_protocol::{
    memo::{Memo, MemoBytes},
    TxId,
};

use crate::config::db_path;
use crate::error::{Error, Result};
use crate::kit::kit_path;
use crate::wallet::Wallet;

#[derive(Clone, Debug, Serialize)]
pub struct TxSummary {
    pub txid: String,
    /// `None` while unmined.
    pub mined_height: Option<u32>,
    /// Unmined and past its expiry height: it will never be mined.
    pub expired: bool,
    /// Change in the wallet's balance, fee included.
    pub net_zat: i64,
    pub fee_zat: Option<u64>,
    /// The first text memo among the transaction's outputs.
    pub memo: Option<String>,
    /// Whether a disclosure kit is saved for it.
    pub kit: bool,
}

impl Wallet {
    /// The `limit` most recent transactions, newest (and unmined) first.
    pub fn history(&self, limit: usize) -> Result<Vec<TxSummary>> {
        let conn = Connection::open_with_flags(db_path(&self.dir), OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(Error::db)?;
        rusqlite::vtab::array::load_module(&conn).map_err(Error::db)?;
        let mut txs = conn
            .prepare(
                "SELECT txid, mined_height, account_balance_delta, fee_paid, expired_unmined
                 FROM v_transactions
                 ORDER BY COALESCE(mined_height, NULLIF(expiry_height, 0)) DESC NULLS FIRST
                 LIMIT :limit",
            )
            .map_err(Error::db)?;
        let mut memos = conn
            .prepare("SELECT memo FROM v_tx_outputs WHERE txid = :txid AND memo IS NOT NULL ORDER BY output_index")
            .map_err(Error::db)?;

        let rows = txs
            .query_map(named_params! { ":limit": limit as i64 }, |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<u32>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<bool>>(4)?,
                ))
            })
            .map_err(Error::db)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::db)?;

        rows.into_iter()
            .map(|(raw_txid, mined_height, net_zat, fee, expired)| {
                let txid = TxId::from_bytes(raw_txid.as_slice().try_into().map_err(|_| Error::Database("malformed txid".into()))?);
                let memo = first_text_memo(&mut memos, &raw_txid)?;
                Ok(TxSummary {
                    txid: txid.to_string(),
                    mined_height,
                    expired: expired.unwrap_or(false),
                    net_zat,
                    fee_zat: fee.map(|f| f as u64),
                    memo,
                    kit: kit_path(&self.dir, &txid.to_string()).exists(),
                })
            })
            .collect()
    }
}

fn first_text_memo(stmt: &mut rusqlite::Statement<'_>, txid: &[u8]) -> Result<Option<String>> {
    let mut rows = stmt.query(named_params! { ":txid": txid }).map_err(Error::db)?;
    while let Some(row) = rows.next().map_err(Error::db)? {
        let bytes: Vec<u8> = row.get(0).map_err(Error::db)?;
        if let Some(Memo::Text(text)) = MemoBytes::from_bytes(&bytes).ok().and_then(|m| Memo::try_from(m).ok()) {
            return Ok(Some(text.to_string()));
        }
    }
    Ok(None)
}

