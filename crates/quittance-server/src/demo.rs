//! The live demo: a real mainnet wallet that answers any challenge with a fresh
//! disclosure of one of its past payments, and the wallet that received those
//! payments, which tries (and fails) to disclose them as if it had sent them.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use orchard::circuit::ProvingKey;
use quittance_wallet::{parse_txid, DisclosureRequest, Lightwalletd, Wallet};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use zip311::{Network, OutputSelection};

/// One payment the demo wallet made, as listed in `demo.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DemoPayment {
    pub txid: String,
    pub height: u32,
    pub value_zec: String,
    pub memo: String,
    pub recipient: String,
}

#[derive(Debug, Deserialize)]
struct DemoFile {
    payments: Vec<DemoPayment>,
    /// A real Orchard-pool transaction, for showing that refusal.
    orchard: Option<OrchardTx>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OrchardTx {
    txid: String,
    height: u32,
}

pub struct Demo {
    network: Network,
    servers: Vec<String>,
    sender_dir: PathBuf,
    payee_dir: PathBuf,
    payments: Vec<DemoPayment>,
    orchard: Option<OrchardTx>,
    pk: Arc<ProvingKey>,
    /// One proof at a time: proving is CPU-bound and the machine is small.
    busy: Mutex<()>,
}

impl Demo {
    pub fn load(data: &Path, sender_dir: PathBuf, payee_dir: PathBuf, servers: Vec<String>, pk: Arc<ProvingKey>) -> Result<Self, String> {
        let file: DemoFile = serde_json::from_slice(&std::fs::read(data.join("demo.json")).map_err(|e| format!("demo.json: {e}"))?)
            .map_err(|e| format!("demo.json: {e}"))?;
        if file.payments.is_empty() {
            return Err("demo.json lists no payments".into());
        }
        let network = Wallet::open(&sender_dir).map_err(|e| e.to_string())?.network();
        Ok(Demo { network, servers, sender_dir, payee_dir, payments: file.payments, orchard: file.orchard, pk, busy: Mutex::new(()) })
    }

    pub fn info(&self) -> Value {
        json!({ "network": self.network, "payments": self.payments, "orchard_txid": self.orchard.as_ref().map(|o| &o.txid) })
    }

    fn payment(&self, i: usize) -> Result<&DemoPayment, String> {
        self.payments.get(i).ok_or_else(|| format!("the demo has {} payments", self.payments.len()))
    }

    /// A fresh disclosure of payment `i`, signed over `msg`, with the
    /// transaction attached so the browser needn't look it up by txid.
    pub async fn prove(&self, msg: Vec<u8>, i: usize) -> Result<Value, String> {
        let payment = self.payment(i)?.clone();
        let txid = parse_txid(&payment.txid)?;
        let _turn = self.busy.lock().await;
        let started = Instant::now();
        let (dir, network, servers, pk) = (self.sender_dir.clone(), self.network, self.servers.clone(), self.pk.clone());
        // The wallet's database handle isn't Sync, so the proof runs on a
        // blocking thread that drives its own future to completion.
        let d = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let wallet = Wallet::open(&dir).map_err(|e| e.to_string())?;
                let mut lwd = Lightwalletd::connect(network, &servers).await.map_err(|e| e.to_string())?;
                let req = DisclosureRequest { txid, msg, outputs: OutputSelection::Payments, embed_tx: true };
                wallet.prove(&mut lwd, req, &pk).await.map_err(|e| e.to_string())
            })
        })
        .await
        .map_err(|e| e.to_string())??;
        Ok(json!({ "disclosure": d.encode(), "payment": payment, "prove_ms": started.elapsed().as_millis() as u64 }))
    }

    /// The recipient of payment `i` tries to disclose it as its sender. Its
    /// wallet refuses before any proof exists: it spent nothing in that
    /// transaction, so it has no note to re-prove.
    pub async fn forge_as_recipient(&self, i: usize) -> Result<Value, String> {
        let payment = self.payment(i)?.clone();
        let txid = parse_txid(&payment.txid)?;
        let _turn = self.busy.lock().await;
        let (dir, network, servers) = (self.payee_dir.clone(), self.network, self.servers.clone());
        let attempt = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let mut payee = Wallet::open(&dir).map_err(|e| e.to_string())?;
                let mut lwd = Lightwalletd::connect(network, &servers).await.map_err(|e| e.to_string())?;
                Ok::<_, String>(payee.capture_kit(&mut lwd, txid).await.map(|_| ()).map_err(|e| e.to_string()))
            })
        })
        .await
        .map_err(|e| e.to_string())??;
        Ok(match attempt {
            Err(e) => json!({ "refused": true, "reason": e.to_string(), "payment": payment }),
            Ok(_) => json!({ "refused": false, "reason": "the recipient's wallet produced a kit, which should never happen", "payment": payment }),
        })
    }

    /// The same disclosure, retargeted at a real Orchard-pool transaction. The
    /// verifier refuses it at the pool check.
    pub fn orchard_variant(&self, text: &str) -> Result<String, String> {
        let o = self.orchard.as_ref().ok_or("no Orchard transaction is configured")?;
        let mut d = zip311::PaymentDisclosure::decode(text.trim()).map_err(|e| e.to_string())?;
        let mut txid: [u8; 32] = hex::decode(&o.txid).ok().and_then(|b| b.try_into().ok()).ok_or("bad Orchard txid")?;
        txid.reverse();
        d.txid = txid;
        d.height = o.height;
        d.tx = None;
        Ok(d.encode())
    }
}
