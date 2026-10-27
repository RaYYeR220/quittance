//! Conformance against real Zcash mainnet transactions: three Ironwood
//! payments, each disclosed by its sender. The vectors hold the raw
//! transaction as the chain served it, so these tests need no network.

use std::sync::OnceLock;

use orchard::circuit::VerifyingKey;
use serde_json::Value;
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::{BlockHeight, BranchId, MAIN_NETWORK};
use zip311::{CheckStatus, MemoView, Network, PaymentDisclosure, Verdict, VerifyContext};

const VECTORS: [&str; 3] = [
    include_str!("../../../test-vectors/mainnet/payment1.json"),
    include_str!("../../../test-vectors/mainnet/payment2.json"),
    include_str!("../../../test-vectors/mainnet/payment3.json"),
];

/// Before every challenge in the vectors expires.
const NOW: u64 = 1_793_100_000;

fn vk() -> &'static VerifyingKey {
    static VK: OnceLock<VerifyingKey> = OnceLock::new();
    VK.get_or_init(zip311::verifying_key)
}

struct Vector {
    v: Value,
    tx: Transaction,
    height: u32,
}

fn vectors() -> Vec<Vector> {
    VECTORS
        .iter()
        .map(|s| {
            let v: Value = serde_json::from_str(s).unwrap();
            let height = v["mined_height"].as_u64().unwrap() as u32;
            let raw = hex::decode(v["raw_tx"].as_str().unwrap()).unwrap();
            let tx = Transaction::read(&raw[..], BranchId::for_height(&MAIN_NETWORK, BlockHeight::from_u32(height))).unwrap();
            Vector { v, tx, height }
        })
        .collect()
}

fn check(d: &PaymentDisclosure, x: &Vector, expected: Option<&str>, height: Option<u32>) -> Verdict {
    let cx = VerifyContext { network: Network::Main, expected_msg: expected.map(str::as_bytes), vk: vk(), now: NOW };
    zip311::verify(d, &x.tx, height, &cx)
}

fn failed(v: &Verdict) -> Vec<&'static str> {
    v.checks.iter().filter(|c| c.status == CheckStatus::Fail).map(|c| c.id).collect()
}

fn text<'a>(x: &'a Vector, key: &str) -> &'a str {
    x.v[key].as_str().unwrap()
}

#[test]
fn every_mainnet_receipt_proves_its_payment() {
    for x in vectors() {
        let d = PaymentDisclosure::decode(text(&x, "disclosure")).unwrap();
        assert_eq!(d.txid_display(), text(&x, "txid"));
        let v = check(&d, &x, Some(text(&x, "challenge")), Some(x.height));
        assert!(v.valid, "{}: {:#?}", text(&x, "txid"), v.checks);
        let p = &v.revealed.as_ref().unwrap().payments[0];
        let e = &x.v["expected"];
        assert_eq!(p.value_zat, e["value_zat"].as_u64().unwrap());
        assert_eq!(p.recipient, e["recipient"].as_str().unwrap());
        assert!(matches!(&p.memo, MemoView::Text(m) if m == e["memo"].as_str().unwrap()));
    }
}

#[test]
fn the_encoding_is_canonical() {
    for x in vectors() {
        for key in ["disclosure", "disclosure_with_tx"] {
            assert_eq!(PaymentDisclosure::decode(text(&x, key)).unwrap().encode(), text(&x, key));
        }
    }
}

#[test]
fn the_attached_transaction_is_the_mined_one() {
    for x in vectors() {
        let d = PaymentDisclosure::decode(text(&x, "disclosure_with_tx")).unwrap();
        assert_eq!(hex::encode(d.tx.as_ref().unwrap()), text(&x, "raw_tx"));
        assert!(check(&d, &x, Some(text(&x, "challenge")), Some(x.height)).valid);
    }
}

#[test]
fn a_receipt_checked_against_another_challenge_is_refused() {
    for x in vectors() {
        let d = PaymentDisclosure::decode(text(&x, "disclosure")).unwrap();
        let v = check(&d, &x, Some(text(&x, "other_challenge")), Some(x.height));
        assert_eq!(failed(&v), vec!["challenge"]);
    }
}

#[test]
fn tampering_with_a_real_receipt_is_refused() {
    for x in vectors() {
        let d = PaymentDisclosure::decode(text(&x, "disclosure")).unwrap();
        let ok = Some(text(&x, "challenge"));

        let mut t = d.clone();
        t.outputs[0].ock[3] ^= 1;
        assert_eq!(failed(&check(&t, &x, ok, Some(x.height))), vec!["spend-auth"]);

        let mut t = d.clone();
        let proof = &mut t.reproved.as_mut().unwrap().proof;
        proof[100] ^= 1;
        assert_eq!(failed(&check(&t, &x, ok, Some(x.height))), vec!["spend-proof"]);

        let mut t = d.clone();
        t.msg = text(&x, "other_challenge").as_bytes().to_vec();
        assert_eq!(failed(&check(&t, &x, Some(text(&x, "other_challenge")), Some(x.height))), vec!["spend-auth"]);

        assert_eq!(failed(&check(&d, &x, ok, Some(x.height + 1))), vec!["mined"]);
        assert_eq!(failed(&check(&d, &x, ok, None)), vec!["mined"]);
    }
}

#[test]
fn a_receipt_does_not_verify_against_another_payment() {
    let xs = vectors();
    let d = PaymentDisclosure::decode(text(&xs[0], "disclosure")).unwrap();
    let v = check(&d, &xs[1], Some(text(&xs[0], "challenge")), Some(xs[1].height));
    assert_eq!(failed(&v), vec!["txid"]);
}
