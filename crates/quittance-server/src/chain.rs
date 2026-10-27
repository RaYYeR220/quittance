//! Reading transactions from lightwalletd, with endpoint failover and a cache.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use zcash_client_backend::proto::service::{compact_tx_streamer_client::CompactTxStreamerClient, BlockId, ChainSpec, TxFilter};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::{BlockHeight, BranchId};
use zip311::Network;

const TIMEOUT: Duration = Duration::from_secs(12);
const CACHE_CAP: usize = 1024;
/// A mined transaction is re-read after this long, so a reorg can't pin a
/// stale height; an unmined or unknown one sooner.
const MINED_TTL: Duration = Duration::from_secs(120);
const UNMINED_TTL: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    #[error("the chain has no transaction with this txid")]
    NotFound,
    #[error("no lightwalletd server answered: {0}")]
    Unavailable(String),
    #[error("the transaction could not be parsed: {0}")]
    Parse(String),
}

/// A transaction as the chain holds it.
#[derive(Clone)]
pub struct ChainTx {
    pub raw: Vec<u8>,
    /// `None` while it's only in the mempool or on a side chain.
    pub height: Option<u32>,
}

impl ChainTx {
    /// Parses the transaction under the consensus rules of the height it was
    /// mined at (or `fallback` for an unmined one).
    pub fn parse(&self, network: Network, fallback: u32) -> Result<Transaction, ChainError> {
        let h = BlockHeight::from_u32(self.height.unwrap_or(fallback));
        let branch = BranchId::for_height(&network.params(), h);
        Transaction::read(&self.raw[..], branch).map_err(|e| ChainError::Parse(e.to_string()))
    }
}

pub struct Chain {
    channels: HashMap<Network, Vec<(String, Channel)>>,
    /// `None` caches a not-found answer, briefly.
    cache: Mutex<HashMap<(Network, [u8; 32]), (Option<ChainTx>, Instant)>>,
}

impl Chain {
    pub fn new(endpoints: &HashMap<Network, Vec<String>>) -> Result<Self, String> {
        let mut channels = HashMap::new();
        for (net, urls) in endpoints {
            let mut list = vec![];
            for url in urls {
                let ep = Endpoint::from_shared(url.clone())
                    .map_err(|e| format!("{url}: {e}"))?
                    .connect_timeout(TIMEOUT)
                    .timeout(TIMEOUT)
                    .tls_config(ClientTlsConfig::new().with_webpki_roots())
                    .map_err(|e| format!("{url}: {e}"))?;
                list.push((url.clone(), ep.connect_lazy()));
            }
            channels.insert(*net, list);
        }
        Ok(Chain { channels, cache: Mutex::new(HashMap::new()) })
    }

    fn clients(&self, network: Network) -> impl Iterator<Item = (&str, CompactTxStreamerClient<Channel>)> {
        self.channels
            .get(&network)
            .into_iter()
            .flatten()
            .map(|(url, ch)| (url.as_str(), CompactTxStreamerClient::new(ch.clone())))
    }

    /// Looks a transaction up by txid (as serialized, not display order).
    pub async fn transaction(&self, network: Network, txid: [u8; 32]) -> Result<ChainTx, ChainError> {
        if let Some((tx, at)) = self.cache.lock().unwrap().get(&(network, txid)) {
            let ttl = if tx.as_ref().is_some_and(|t| t.height.is_some()) { MINED_TTL } else { UNMINED_TTL };
            if at.elapsed() < ttl {
                return tx.clone().ok_or(ChainError::NotFound);
            }
        }
        let mut errors = vec![];
        for (url, mut client) in self.clients(network) {
            let req = TxFilter { block: None, index: 0, hash: txid.to_vec() };
            match client.get_transaction(req).await {
                Ok(resp) => {
                    let r = resp.into_inner();
                    let height = match r.height {
                        0 | u64::MAX => None,
                        h => u32::try_from(h).ok(),
                    };
                    let tx = ChainTx { raw: r.data, height };
                    self.remember(network, txid, Some(tx.clone()));
                    return Ok(tx);
                }
                Err(status)
                    if status.code() == tonic::Code::NotFound
                        || status.message().contains("not found")
                        || status.message().to_lowercase().contains("no such") =>
                {
                    self.remember(network, txid, None);
                    return Err(ChainError::NotFound);
                }
                Err(status) => errors.push(format!("{url}: {}", status.message())),
            }
        }
        Err(ChainError::Unavailable(errors.join("; ")))
    }

    fn remember(&self, network: Network, txid: [u8; 32], tx: Option<ChainTx>) {
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_CAP {
            cache.clear();
        }
        cache.insert((network, txid), (tx, Instant::now()));
    }

    /// The txids (display order) of the block at `height`. Lets a verifier that
    /// already holds a transaction confirm it is mined there without telling
    /// the server which transaction it is interested in.
    pub async fn block_txids(&self, network: Network, height: u32) -> Result<Vec<String>, ChainError> {
        let mut errors = vec![];
        for (url, mut client) in self.clients(network) {
            match client.get_block(BlockId { height: height as u64, hash: vec![] }).await {
                Ok(r) => {
                    return Ok(r
                        .into_inner()
                        .vtx
                        .iter()
                        .map(|t| {
                            let mut h = t.txid.clone();
                            h.reverse();
                            hex::encode(h)
                        })
                        .collect())
                }
                Err(s) => errors.push(format!("{url}: {}", s.message())),
            }
        }
        Err(ChainError::Unavailable(errors.join("; ")))
    }

    pub async fn tip(&self, network: Network) -> Result<u32, ChainError> {
        let mut errors = vec![];
        for (url, mut client) in self.clients(network) {
            match client.get_latest_block(ChainSpec {}).await {
                Ok(r) => return Ok(r.into_inner().height as u32),
                Err(s) => errors.push(format!("{url}: {}", s.message())),
            }
        }
        Err(ChainError::Unavailable(errors.join("; ")))
    }
}
