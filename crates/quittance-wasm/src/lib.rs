//! Verifying payment disclosures in the browser, so a verifier never has to
//! send a disclosure to anyone else's server.
//!
//! The caller fetches the transaction (from any lightwalletd proxy, or from the
//! disclosure itself if the prover attached it) and its mined height; this
//! module does the rest. Results are returned as JSON strings.

use std::sync::OnceLock;

use orchard::circuit::VerifyingKey;
use serde_json::json;
use wasm_bindgen::prelude::*;
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::{BlockHeight, BranchId};
use zip311::{Network, PaymentDisclosure, VerifyContext, NOT_REVEALED};

static VK: OnceLock<VerifyingKey> = OnceLock::new();

fn vk() -> &'static VerifyingKey {
    VK.get_or_init(zip311::verifying_key)
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// Accepts the bare `zpd1…` text or a link whose fragment carries it.
fn decode(text: &str) -> Result<PaymentDisclosure, JsError> {
    let text = text.trim();
    let text = text.rsplit_once('#').map(|(_, t)| t).unwrap_or(text);
    PaymentDisclosure::decode(text).map_err(err)
}

/// Builds the verifying key ahead of time (it takes a moment); optional.
#[wasm_bindgen]
pub fn prepare() {
    vk();
}

/// What a disclosure claims, before anything is checked.
#[wasm_bindgen]
pub fn inspect(text: &str) -> Result<String, JsError> {
    let d = decode(text)?;
    let summary = json!({
        "network": d.network,
        "txid": d.txid_display(),
        "height": d.height,
        "challenge_text": std::str::from_utf8(&d.msg).ok(),
        "challenge": zip311::Challenge::parse(&d.msg),
        "spends": d.reproved.as_ref().map(|r| r.actions.len()).unwrap_or(0) + d.resigned.len(),
        "outputs": d.outputs.len(),
        "proof_bytes": d.reproved.as_ref().map(|r| r.proof.len()).unwrap_or(0),
        "has_tx": d.tx.is_some(),
        "bytes": d.to_bytes().len(),
    });
    Ok(summary.to_string())
}

/// Verifies `text` against the transaction `raw_tx_hex` mined at `mined_height`
/// (`undefined` if the chain has it only in the mempool or not at all). An
/// empty `raw_tx_hex` uses the transaction attached to the disclosure.
#[wasm_bindgen]
pub fn verify(
    text: &str,
    raw_tx_hex: &str,
    mined_height: Option<u32>,
    expected_challenge: Option<String>,
    network: Option<String>,
    now_unix: f64,
) -> Result<String, JsError> {
    let d = decode(text)?;
    let network = match network.as_deref() {
        None | Some("") => d.network,
        Some("main") | Some("mainnet") => Network::Main,
        Some("test") | Some("testnet") => Network::Test,
        Some(other) => return Err(err(format!("unknown network {other}"))),
    };
    let raw = if raw_tx_hex.trim().is_empty() {
        d.tx.clone().ok_or_else(|| err("no transaction given and none attached to the disclosure"))?
    } else {
        hex::decode(raw_tx_hex.trim()).map_err(err)?
    };
    let h = BlockHeight::from_u32(mined_height.unwrap_or(d.height));
    let tx = Transaction::read(&raw[..], BranchId::for_height(&network.params(), h)).map_err(err)?;
    let expected = expected_challenge.map(String::into_bytes);
    let cx = VerifyContext { network, expected_msg: expected.as_deref(), vk: vk(), now: now_unix.max(0.0) as u64 };
    let verdict = zip311::verify(&d, &tx, mined_height, &cx);
    let mut v = serde_json::to_value(verdict).map_err(err)?;
    v["not_revealed"] = json!(NOT_REVEALED);
    Ok(v.to_string())
}

/// A deliberately broken copy of a disclosure, for showing what refusals look
/// like: `output-key`, `proof`, `signature`, or `challenge` (rewrites the
/// challenge text without re-signing).
#[wasm_bindgen]
pub fn tamper(text: &str, how: &str, new_challenge: Option<String>) -> Result<String, JsError> {
    let mut d = decode(text)?;
    match how {
        "output-key" => {
            let o = d.outputs.first_mut().ok_or_else(|| err("no output to tamper with"))?;
            o.ock[0] ^= 0x01;
        }
        "proof" => {
            let r = d.reproved.as_mut().ok_or_else(|| err("no proof to tamper with"))?;
            let mid = r.proof.len() / 2;
            r.proof[mid] ^= 0x01;
        }
        "signature" => {
            let r = d.reproved.as_mut().ok_or_else(|| err("no signature to tamper with"))?;
            r.actions[0].spend_auth_sig[7] ^= 0x01;
        }
        "challenge" => {
            d.msg = new_challenge.unwrap_or_else(|| "a challenge this disclosure was never made for".into()).into_bytes();
        }
        other => return Err(err(format!("unknown tampering `{other}`"))),
    }
    Ok(d.encode())
}
