//! End-to-end disclosure tests over synthetic Ironwood bundles: a payment is
//! built and proven exactly as a wallet would, then disclosed and verified, and
//! every way of lying about it is checked to be refused for the right reason.

use std::sync::OnceLock;

use incrementalmerkletree::{Hashable, Retention};
use orchard::{
    builder::{Builder, BundleType},
    bundle::{Authorized, BundleVersion, Flags, TxVersion},
    circuit::{ProvingKey, VerifyingKey},
    keys::{FullViewingKey, PreparedIncomingViewingKey, Scope, SpendAuthorizingKey, SpendingKey},
    note::ExtractedNoteCommitment,
    note_encryption::IronwoodDomain,
    tree::{MerkleHashOrchard, MerklePath},
    value::NoteValue,
    Address, Anchor, Bundle, Note,
};
use rand::{rand_core::UnwrapErr, rngs::SysRng, Rng};
use shardtree::{store::memory::MemoryShardStore, ShardTree};
use zcash_note_encryption::try_note_decryption;
use zip311::{
    prove_bundle, verify_bundle, CheckStatus, MemoView, Network, OutputSelection, PaymentDisclosure, ProveError,
    SpendWitness, Verdict, VerifyContext,
};

fn os_rng() -> UnwrapErr<SysRng> {
    UnwrapErr(SysRng)
}

fn keys() -> &'static (ProvingKey, VerifyingKey) {
    static KEYS: OnceLock<(ProvingKey, VerifyingKey)> = OnceLock::new();
    KEYS.get_or_init(|| (zip311::proving_key(), zip311::verifying_key()))
}

fn spending_key(rng: &mut impl Rng) -> SpendingKey {
    loop {
        let mut b = [0u8; 32];
        rng.fill_bytes(&mut b);
        if let Some(sk) = Option::from(SpendingKey::from_bytes(b)) {
            return sk;
        }
    }
}

struct Wallet {
    fvk: FullViewingKey,
    ask: SpendAuthorizingKey,
    addr: Address,
}

impl Wallet {
    fn new(rng: &mut impl Rng) -> Self {
        let sk = spending_key(rng);
        let fvk = FullViewingKey::from(&sk);
        Wallet { addr: fvk.address_at(0u32, Scope::External), ask: SpendAuthorizingKey::from(&sk), fvk }
    }

    fn find_note(&self, bundle: &Bundle<Authorized, i64>) -> Note {
        let ivk = PreparedIncomingViewingKey::new(&self.fvk.to_ivk(Scope::External));
        bundle
            .actions()
            .iter()
            .find_map(|a| try_note_decryption(&IronwoodDomain::for_action(a), &ivk, a))
            .map(|(n, _, _)| n)
            .expect("the wallet received a note")
    }
}

fn memo(text: &str) -> [u8; 512] {
    let mut m = [0u8; 512];
    m[..text.len()].copy_from_slice(text.as_bytes());
    m
}

fn finish(builder: Builder) -> Bundle<Authorized, i64> {
    finish_with(builder, &[])
}

fn finish_with(builder: Builder, asks: &[SpendAuthorizingKey]) -> Bundle<Authorized, i64> {
    let mut rng = os_rng();
    let (u, _) = builder.build::<i64>(&mut rng).unwrap().unwrap();
    let sighash: [u8; 32] = u.commitment(TxVersion::V6).unwrap().into();
    u.create_proof(&keys().0, &mut rng).unwrap().apply_signatures(rng, sighash, asks).unwrap()
}

fn empty_anchor() -> Anchor {
    MerkleHashOrchard::empty_root(32.into()).into()
}

fn fund(to: Address, value: u64) -> Bundle<Authorized, i64> {
    let iw = BundleVersion::ironwood_v3();
    let mut b = Builder::new(BundleType::DEFAULT, iw, Flags::SPENDS_DISABLED, empty_anchor()).unwrap();
    b.add_output(None, to, NoteValue::from_raw(value), memo("funding")).unwrap();
    finish(b)
}

/// A chain in miniature: one note commitment tree, checkpointed per "block".
struct Chain {
    tree: ShardTree<MemoryShardStore<MerkleHashOrchard, u32>, 32, 16>,
    block: u32,
}

impl Chain {
    fn new() -> Self {
        Chain { tree: ShardTree::new(MemoryShardStore::empty(), 100), block: 0 }
    }

    fn mine(&mut self, bundle: &Bundle<Authorized, i64>, mark: &[Note]) {
        let marked: Vec<ExtractedNoteCommitment> = mark.iter().map(|n| n.commitment().into()).collect();
        for a in bundle.actions() {
            let retention = if marked.contains(a.cmx()) { Retention::Marked } else { Retention::Ephemeral };
            self.tree.append(MerkleHashOrchard::from_cmx(a.cmx()), retention).unwrap();
        }
        self.block += 1;
        self.tree.checkpoint(self.block).unwrap();
    }

    fn anchor(&self) -> Anchor {
        self.tree.root_at_checkpoint_id(&self.block).unwrap().unwrap().into()
    }

    fn witness(&self, note: &Note) -> MerklePath {
        let cmx: ExtractedNoteCommitment = note.commitment().into();
        let leaf = MerkleHashOrchard::from_cmx(&cmx);
        for pos in self.tree.marked_positions().unwrap() {
            let path = self.tree.witness_at_checkpoint_id(pos, &self.block).unwrap().unwrap();
            if path.root(leaf) == self.tree.root_at_checkpoint_id(&self.block).unwrap().unwrap() {
                return path.into();
            }
        }
        panic!("note is not marked in the tree");
    }
}

/// Alice pays Bob 30,000 zat with a memo; 19,000 zat change goes back to Alice.
struct Payment {
    alice: Wallet,
    bob: Wallet,
    funding: Bundle<Authorized, i64>,
    note: Note,
    path: MerklePath,
    tx: Bundle<Authorized, i64>,
    txid: [u8; 32],
}

/// One payment shared by every test: proving is the slow part.
fn payment() -> &'static Payment {
    static P: OnceLock<Payment> = OnceLock::new();
    P.get_or_init(make_payment)
}

fn make_payment() -> Payment {
    let mut rng = os_rng();
    let alice = Wallet::new(&mut rng);
    let bob = Wallet::new(&mut rng);
    let funding = fund(alice.addr, 50_000);
    let note = alice.find_note(&funding);
    let mut chain = Chain::new();
    chain.mine(&fund(Wallet::new(&mut rng).addr, 7), &[]);
    chain.mine(&funding, &[note]);
    chain.mine(&fund(Wallet::new(&mut rng).addr, 9), &[]);
    let anchor = chain.anchor();
    let path = chain.witness(&note);

    let iw = BundleVersion::ironwood_v3();
    let mut b = Builder::new(BundleType::DEFAULT, iw, iw.default_flags(), anchor).unwrap();
    b.add_spend(alice.fvk.clone(), note, path.clone()).unwrap();
    b.add_output(Some(alice.fvk.to_ovk(Scope::External)), bob.addr, NoteValue::from_raw(30_000), memo("invoice 42 / acme"))
        .unwrap();
    b.add_output(
        Some(alice.fvk.to_ovk(Scope::Internal)),
        alice.fvk.address_at(0u32, Scope::Internal),
        NoteValue::from_raw(19_000),
        memo(""),
    )
    .unwrap();
    let tx = finish_with(b, std::slice::from_ref(&alice.ask));
    let mut txid = [0u8; 32];
    rng.fill_bytes(&mut txid);
    Payment { alice, bob, funding, note, path, tx, txid }
}

const HEIGHT: u32 = 4_500_123;
const CHALLENGE: &[u8] = b"quittance:v1|Acme Exchange|deposit dispute 8812|00112233445566778899aabbccddeeff|4102444800";
const NOW: u64 = 1_793_000_000;

fn disclose(p: &Payment, msg: &[u8], outputs: OutputSelection) -> Result<PaymentDisclosure, ProveError> {
    let w = SpendWitness { note: p.note, merkle_path: p.path.clone() };
    prove_bundle(Network::Test, p.txid, &p.tx, HEIGHT, msg.to_vec(), &p.alice.fvk, &p.alice.ask, &[w], &outputs, &keys().0, &mut os_rng())
}

fn check(d: &PaymentDisclosure, p: &Payment, expected: Option<&[u8]>) -> Verdict {
    let cx = VerifyContext { network: Network::Test, expected_msg: expected, vk: &keys().1, now: NOW };
    verify_bundle(d, p.txid, Some(&p.tx), false, Some(HEIGHT), &cx)
}

fn failed(v: &Verdict) -> Vec<&'static str> {
    v.checks.iter().filter(|c| c.status == CheckStatus::Fail).map(|c| c.id).collect()
}

#[test]
fn an_honest_disclosure_proves_the_payment_and_nothing_else() {
    let p = payment();
    let d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    let v = check(&d, p, Some(CHALLENGE));
    assert!(v.valid, "{:#?}", v.checks);
    assert!(v.bound && v.fresh);
    assert!(failed(&v).is_empty());
    let r = v.revealed.as_ref().unwrap();
    assert_eq!(r.payments.len(), 1, "change is not disclosed by default");
    let pay = &r.payments[0];
    assert_eq!(pay.value_zat, 30_000);
    assert_eq!(pay.value_zec, "0.0003");
    assert!(matches!(pay.memo, MemoView::Text(ref m) if m == "invoice 42 / acme"));
    assert!(pay.recipient.starts_with("utest1"));
    assert_eq!(r.spends.len(), 1);
    assert_eq!(r.spends[0].nullifier, hex::encode(p.note.nullifier(&p.alice.fvk).to_bytes()));
    assert_eq!(v.challenge.as_ref().unwrap().verifier, "Acme Exchange");
}

#[test]
fn the_text_form_round_trips_and_still_verifies() {
    let p = payment();
    let d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    let text = d.encode();
    assert!(text.starts_with("zpdtest1"));
    let back = PaymentDisclosure::decode(&text).unwrap();
    assert_eq!(back, d);
    assert!(check(&back, p, Some(CHALLENGE)).valid);
}

#[test]
fn a_disclosure_made_for_another_verifier_is_refused() {
    let p = payment();
    let d = disclose(p, b"quittance:v1|Someone else|x|ffffffffffffffffffffffffffffffff|4102444800", OutputSelection::Payments).unwrap();
    let v = check(&d, p, Some(CHALLENGE));
    assert!(!v.valid);
    assert_eq!(failed(&v), vec!["challenge"]);
}

#[test]
fn rewriting_the_challenge_breaks_the_signature() {
    let p = payment();
    let mut d = disclose(p, b"some other challenge", OutputSelection::Payments).unwrap();
    d.msg = CHALLENGE.to_vec();
    let v = check(&d, p, Some(CHALLENGE));
    assert!(!v.valid);
    assert_eq!(failed(&v), vec!["spend-auth"]);
}

#[test]
fn without_an_expected_challenge_the_verdict_warns_about_replay() {
    let p = payment();
    let d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    let v = check(&d, p, None);
    assert!(v.valid);
    assert!(!v.bound, "nothing was checked against a challenge of the verifier's");
    let c = v.checks.iter().find(|c| c.id == "challenge").unwrap();
    assert_eq!(c.status, CheckStatus::Warn);
}

#[test]
fn a_tampered_output_key_is_refused() {
    let p = payment();
    let mut d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    d.outputs[0].ock[0] ^= 1;
    assert_eq!(failed(&check(&d, p, Some(CHALLENGE))), vec!["spend-auth"]);
}

#[test]
fn pointing_an_output_key_at_another_action_is_refused() {
    let p = payment();
    let mut d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    let other = (d.outputs[0].tx_action_index + 1) % p.tx.actions().len() as u32;
    d.outputs[0].tx_action_index = other;
    assert!(!check(&d, p, Some(CHALLENGE)).valid);
}

#[test]
fn a_tampered_proof_is_refused() {
    let p = payment();
    let mut d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    let proof = &mut d.reproved.as_mut().unwrap().proof;
    let mid = proof.len() / 2;
    proof[mid] ^= 1;
    assert_eq!(failed(&check(&d, p, Some(CHALLENGE))), vec!["spend-proof"]);
}

#[test]
fn a_tampered_signature_is_refused() {
    let p = payment();
    let mut d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    d.reproved.as_mut().unwrap().actions[0].spend_auth_sig[10] ^= 1;
    assert_eq!(failed(&check(&d, p, Some(CHALLENGE))), vec!["spend-auth"]);
}

#[test]
fn claiming_someone_elses_spend_is_refused() {
    // Point the re-proof at an action whose nullifier the prover never re-proved.
    let p = payment();
    let mut d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    let a = &mut d.reproved.as_mut().unwrap().actions[0];
    a.tx_action_index = (a.tx_action_index + 1) % p.tx.actions().len() as u32;
    assert_eq!(failed(&check(&d, p, Some(CHALLENGE))), vec!["spend-proof"]);
}

#[test]
fn swapping_in_another_spend_key_is_refused() {
    // Keep the proof, replace rk with a key the attacker controls and re-sign:
    // the proof was made for the original rk, so it no longer verifies.
    use orchard::primitives::redpallas::{self, SpendAuth};
    let p = payment();
    let mut d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    let rng = os_rng();
    let mut attacker = [0u8; 32];
    attacker[0] = 7;
    let sk = redpallas::SigningKey::<SpendAuth>::try_from(attacker).unwrap();
    let vk = redpallas::VerificationKey::from(&sk);
    d.reproved.as_mut().unwrap().actions[0].rk = <[u8; 32]>::from(&vk);
    let digest = d.digest();
    d.reproved.as_mut().unwrap().actions[0].spend_auth_sig = <[u8; 64]>::from(&sk.sign(rng, &digest));
    assert_eq!(failed(&check(&d, p, Some(CHALLENGE))), vec!["spend-proof"]);
}

#[test]
fn the_recipient_cannot_disclose_as_the_sender() {
    // Bob received the payment; he can open it, but he can't prove he spent
    // anything in it, because the note he holds was created, not spent, here.
    let p = payment();
    let bob_note = p.bob.find_note(&p.tx);
    let w = SpendWitness { note: bob_note, merkle_path: p.path.clone() };
    let r = prove_bundle(Network::Test, p.txid, &p.tx, HEIGHT, CHALLENGE.to_vec(), &p.bob.fvk, &p.bob.ask, &[w], &OutputSelection::Payments, &keys().0, &mut os_rng());
    assert!(matches!(r, Err(ProveError::NoteNotSpentHere(0))));
}

#[test]
fn a_witness_at_another_anchor_is_refused_before_proving() {
    // The same note, witnessed in a later tree state: right note, wrong root.
    let p = payment();
    let mut later = Chain::new();
    later.mine(&p.funding, &[p.note]);
    later.mine(&fund(p.bob.addr, 5), &[]);
    let w = SpendWitness { note: p.note, merkle_path: later.witness(&p.note) };
    let r = prove_bundle(Network::Test, p.txid, &p.tx, HEIGHT, CHALLENGE.to_vec(), &p.alice.fvk, &p.alice.ask, &[w], &OutputSelection::Payments, &keys().0, &mut os_rng());
    assert!(matches!(r, Err(ProveError::WrongAnchor(0))));
}

#[test]
fn the_disclosure_is_bound_to_its_transaction_network_and_height() {
    let p = payment();
    let d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    let cx = VerifyContext { network: Network::Test, expected_msg: Some(CHALLENGE), vk: &keys().1, now: NOW };

    let mut other_txid = p.txid;
    other_txid[0] ^= 1;
    assert_eq!(failed(&verify_bundle(&d, other_txid, Some(&p.tx), false, Some(HEIGHT), &cx)), vec!["txid"]);
    assert_eq!(failed(&verify_bundle(&d, p.txid, Some(&p.tx), false, None, &cx)), vec!["mined"]);
    assert_eq!(failed(&verify_bundle(&d, p.txid, Some(&p.tx), false, Some(HEIGHT + 1), &cx)), vec!["mined"]);

    let main = VerifyContext { network: Network::Main, ..cx };
    assert_eq!(failed(&verify_bundle(&d, p.txid, Some(&p.tx), false, Some(HEIGHT), &main)), vec!["network"]);
}

#[test]
fn orchard_pool_transactions_are_refused_with_the_reason() {
    let p = payment();
    let d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    let cx = VerifyContext { network: Network::Test, expected_msg: Some(CHALLENGE), vk: &keys().1, now: NOW };
    let v = verify_bundle::<i64>(&d, p.txid, None, true, Some(HEIGHT), &cx);
    assert_eq!(failed(&v), vec!["pool"]);
    let pool = v.checks.iter().find(|c| c.id == "pool").unwrap();
    assert!(pool.detail.contains("soundness"));
}

#[test]
fn change_can_be_disclosed_on_request_but_not_someone_elses_output() {
    let p = payment();
    let all = disclose(p, CHALLENGE, OutputSelection::Indices((0..p.tx.actions().len() as u32).collect()));
    // Every output of this payment was made with Alice's keys, so every one opens.
    let d = all.unwrap();
    let v = check(&d, p, Some(CHALLENGE));
    assert!(v.valid);
    let mut values: Vec<u64> = v.revealed.unwrap().payments.iter().map(|x| x.value_zat).collect();
    values.sort();
    assert_eq!(values, vec![19_000, 30_000]);

    // Bob can't open Alice's outputs with his own keys.
    let w = SpendWitness { note: p.note, merkle_path: p.path.clone() };
    let r = prove_bundle(Network::Test, p.txid, &p.tx, HEIGHT, vec![], &p.bob.fvk, &p.alice.ask, &[w], &OutputSelection::Indices(vec![0]), &keys().0, &mut os_rng());
    assert!(r.is_err());
}

#[test]
fn a_disclosure_must_prove_a_spend() {
    let p = payment();
    let mut d = disclose(p, CHALLENGE, OutputSelection::Payments).unwrap();
    d.reproved = None;
    assert_eq!(failed(&check(&d, p, Some(CHALLENGE))), vec!["spend"]);
}

#[test]
fn an_expired_challenge_is_flagged() {
    let p = payment();
    let stale = b"quittance:v1|Acme|x|00112233445566778899aabbccddeeff|1000";
    let d = disclose(p, stale, OutputSelection::Payments).unwrap();
    let v = check(&d, p, Some(stale));
    assert!(v.valid, "expiry is the verifier's call, reported as a warning");
    assert!(v.bound && !v.fresh);
    assert_eq!(v.checks.iter().find(|c| c.id == "expiry").unwrap().status, CheckStatus::Warn);
}
