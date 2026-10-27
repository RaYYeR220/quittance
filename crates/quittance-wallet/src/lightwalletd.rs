//! The wallet's only network peer: a lightwalletd server over gRPC and TLS.

use std::time::Duration;

use futures_util::TryStreamExt;
use orchard::tree::MerkleHashOrchard;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use zcash_client_backend::{
    data_api::chain::CommitmentTreeRoot,
    proto::{
        compact_formats::CompactBlock,
        service::{self, compact_tx_streamer_client::CompactTxStreamerClient},
    },
};
use zcash_primitives::{merkle_tree::HashSer, transaction::Transaction};
use zcash_protocol::{
    consensus::{BlockHeight, BranchId},
    TxId,
};
use zip311::Network;

use crate::error::{Error, Result};

/// Public servers, tried in order.
pub fn default_servers(network: Network) -> &'static [&'static str] {
    match network {
        Network::Main => &["https://zec.rocks:443", "https://eu.zec.rocks:443", "https://na.zec.rocks:443"],
        Network::Test => &["https://testnet.zec.rocks:443"],
    }
}

/// A transaction as the chain reports it.
pub struct ChainTx {
    pub tx: Transaction,
    pub raw: Vec<u8>,
    /// `None` while the transaction is in the mempool (or no longer in the main chain).
    pub mined_height: Option<u32>,
}

pub struct Lightwalletd {
    client: CompactTxStreamerClient<Channel>,
    url: String,
    network: Network,
}

impl Lightwalletd {
    /// Connects to the first server in `servers` (or the network's defaults)
    /// that answers.
    pub async fn connect(network: Network, servers: &[String]) -> Result<Self> {
        let defaults: Vec<String> = default_servers(network).iter().map(|s| s.to_string()).collect();
        let candidates = if servers.is_empty() { &defaults } else { servers };
        let mut failures = vec![];
        for url in candidates {
            match connect_one(url).await {
                Ok(client) => return Ok(Lightwalletd { client, url: url.clone(), network }),
                Err(e) => failures.push(format!("{url}: {e}")),
            }
        }
        Err(Error::Server(format!("no server answered ({})", failures.join("; "))))
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub(crate) fn client(&mut self) -> &mut CompactTxStreamerClient<Channel> {
        &mut self.client
    }

    pub async fn tip(&mut self) -> Result<BlockHeight> {
        let latest = self.client.get_latest_block(service::ChainSpec::default()).await.map_err(status)?.into_inner();
        u32::try_from(latest.height).map(BlockHeight::from_u32).map_err(|_| Error::Server("tip height out of range".into()))
    }

    /// The note commitment tree state as of the end of block `height`.
    pub async fn tree_state(&mut self, height: BlockHeight) -> Result<service::TreeState> {
        let id = service::BlockId { height: u64::from(height), hash: vec![] };
        Ok(self.client.get_tree_state(id).await.map_err(status)?.into_inner())
    }

    pub async fn blocks(&mut self, range: std::ops::Range<BlockHeight>) -> Result<Vec<CompactBlock>> {
        let id = |h: BlockHeight| service::BlockId { height: u64::from(h), hash: vec![] };
        // The request's end is inclusive.
        let request = service::BlockRange { start: Some(id(range.start)), end: Some(id(range.end - 1)), pool_types: vec![] };
        self.client.get_block_range(request).await.map_err(status)?.into_inner().try_collect().await.map_err(status)
    }

    pub(crate) async fn subtree_roots(&mut self, protocol: service::ShieldedProtocol) -> Result<Vec<CommitmentTreeRoot<MerkleHashOrchard>>> {
        let mut request = service::GetSubtreeRootsArg::default();
        request.set_shielded_protocol(protocol);
        let roots: Vec<service::SubtreeRoot> =
            self.client.get_subtree_roots(request).await.map_err(status)?.into_inner().try_collect().await.map_err(status)?;
        roots
            .into_iter()
            .map(|r| {
                let hash = MerkleHashOrchard::read(&r.root_hash[..]).map_err(Error::server)?;
                Ok(CommitmentTreeRoot::from_parts(BlockHeight::from_u32(r.completing_block_height as u32), hash))
            })
            .collect()
    }

    pub(crate) async fn sapling_subtree_roots(&mut self) -> Result<Vec<CommitmentTreeRoot<sapling::Node>>> {
        let mut request = service::GetSubtreeRootsArg::default();
        request.set_shielded_protocol(service::ShieldedProtocol::Sapling);
        let roots: Vec<service::SubtreeRoot> =
            self.client.get_subtree_roots(request).await.map_err(status)?.into_inner().try_collect().await.map_err(status)?;
        roots
            .into_iter()
            .map(|r| {
                let hash = sapling::Node::read(&r.root_hash[..]).map_err(Error::server)?;
                Ok(CommitmentTreeRoot::from_parts(BlockHeight::from_u32(r.completing_block_height as u32), hash))
            })
            .collect()
    }

    /// Looks a transaction up by txid; `None` if the server has never seen it.
    pub async fn transaction(&mut self, txid: TxId) -> Result<Option<ChainTx>> {
        let filter = service::TxFilter { hash: txid.as_ref().to_vec(), ..Default::default() };
        let raw = match self.client.get_transaction(filter).await {
            Ok(r) => r.into_inner(),
            Err(s) if s.code() == tonic::Code::NotFound => return Ok(None),
            // zcashd-backed servers report an unknown txid as a generic error.
            Err(s) if s.message().contains("No such mempool or blockchain transaction") => return Ok(None),
            Err(s) => return Err(status(s)),
        };
        let mined_height = (raw.height > 0 && raw.height < u64::from(u32::MAX)).then_some(raw.height as u32);
        let branch_height = match mined_height {
            Some(h) => BlockHeight::from_u32(h),
            None => self.tip().await? + 1,
        };
        let tx = Transaction::read(&raw.data[..], BranchId::for_height(&self.network.params(), branch_height))
            .map_err(|e| Error::Server(format!("transaction {txid} does not parse: {e}")))?;
        if tx.txid() != txid {
            return Err(Error::Server(format!("asked for transaction {txid}, got {}", tx.txid())));
        }
        Ok(Some(ChainTx { tx, raw: raw.data, mined_height }))
    }

    pub async fn broadcast(&mut self, tx: &Transaction) -> Result<()> {
        let mut data = vec![];
        tx.write(&mut data).map_err(|e| Error::Build(e.to_string()))?;
        let response = self.client.send_transaction(service::RawTransaction { data, height: 0 }).await.map_err(status)?.into_inner();
        if response.error_code != 0 {
            return Err(Error::Broadcast { code: response.error_code, message: response.error_message });
        }
        Ok(())
    }
}

async fn connect_one(url: &str) -> std::result::Result<CompactTxStreamerClient<Channel>, String> {
    let uri: tonic::transport::Uri = url.parse().map_err(|e| format!("bad URL ({e})"))?;
    let mut endpoint = Endpoint::from(uri.clone()).connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(120));
    if uri.scheme_str() == Some("https") {
        let host = uri.host().ok_or("URL has no host")?.to_string();
        let tls = ClientTlsConfig::new().domain_name(host).assume_http2(true).with_webpki_roots();
        endpoint = endpoint.tls_config(tls).map_err(|e| e.to_string())?;
    }
    let channel = endpoint.connect().await.map_err(|e| describe(&e))?;
    Ok(CompactTxStreamerClient::new(channel).max_decoding_message_size(32 << 20))
}

fn status(s: tonic::Status) -> Error {
    Error::Server(format!("{:?}: {}", s.code(), s.message()))
}

/// tonic's transport errors print as "transport error"; the cause is in the source chain.
fn describe(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        text.push_str(": ");
        text.push_str(&s.to_string());
        source = s.source();
    }
    text
}
