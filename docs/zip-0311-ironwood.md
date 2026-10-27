# ZIP 311 for Ironwood: payment disclosures for the post-NU6.3 shielded pool

```
Extends:  ZIP 311 (Zcash Payment Disclosures), Draft
Status:   Proposal, with a reference implementation in this repository (crates/zip311)
Pools:    Ironwood (NU6.3+). Orchard: refused (see "Orchard"). Sapling: as in ZIP 311.
```

## Abstract

ZIP 311 defines how the sender of a Sapling transaction proves that it paid a
given address, without revealing a viewing key. Its Orchard section is a TODO,
it defines no encoding, and its reference implementation is "TBD". Since NU6.3
nearly all shielded value sits in the Ironwood pool, so today there is no way
to make a ZIP 311 disclosure about most shielded payments.

This document specifies the Ironwood case, an encoding, and a challenge
convention, following ZIP 311's Sapling construction as closely as the action
model allows. The key observation is that a disclosure can itself be an
Ironwood bundle: the proof system, signature scheme and verifying key are the
ones consensus already uses, so no new circuit and no trusted setup are needed.

## Motivation

Today a person who must prove a shielded payment (to an exchange's support
desk, a merchant, a grant committee or an auditor) has two options:

* hand over a viewing key, which reveals their entire history, past and future; or
* hand over a txid, which proves nothing, since anyone can point at a transaction.

A payment disclosure reveals one payment and proves who made it: "the holder of
the spending key that spent nullifier `nf` in transaction `T` paid `v` ZEC to
address `A` with memo `M`, and signed your challenge `C`."

## Terminology

`T` is the disclosed transaction, mined at height `h`. `B` is its Ironwood
bundle, with anchor `rt` and actions `B.actions[0..n]`. `fvk`, `ask`, `ovk` are
the prover's Orchard-style full viewing key, spend authorizing key and
outgoing viewing key (Ironwood uses the same key structure as Orchard).

## Specification

### Disclosure structure

| Field | Meaning |
|---|---|
| `network` | mainnet or testnet; fixes the coin type in the digest |
| `txid` | `T`'s txid (ZIP 244) |
| `height` | the height `T` was mined at, as claimed by the prover |
| `msg` | the verifier's challenge (arbitrary bytes, at most 1024) |
| `ironwoodReprovedSpends` | `(actionIndex, cv', rk', cmx', spendAuthSig)` per spend, plus one proof `π` |
| `ironwoodResignedSpends` | `(actionIndex, spendAuthSig)` per spend |
| `ironwoodOutputs` | `(actionIndex, ock)` per disclosed output |

A disclosure MUST contain at least one spend (re-proved or re-signed). As in
ZIP 311, an unsigned disclosure could otherwise have its signatures stripped
and still appear to verify.

### Spend authority, re-proved (the general case)

The prover holds, for each note `n_i` that `T` spent, the note and a Merkle path
from its commitment to `rt`. It builds an Ironwood bundle `D` with:

* bundle version Ironwood v3 and circuit version PostNu6_3;
* anchor `rt`, the anchor of `T`'s own Ironwood bundle;
* flags `enableSpends = 1`, `enableOutputs = 0`, cross-address transfers enabled
  (the Ironwood default). Because outputs are disabled the circuit constrains
  every output value of `D` to zero, so `D` moves no value;
* no padding: exactly one action per disclosed spend;
* fresh `α_i` and `rcv_i` for every action, so `rk'_i` and `cv'_i` are
  unlinkable to `T`'s `rk_i` and `cv_i`.

Each action of `D` spends `n_i` again and therefore carries `T`'s nullifier
`nf_i`. The prover creates the Halo2 proof `π` for `D`, computes the digest
(below), and signs it with `ask` randomized by `α_i`, exactly as it would sign a
sighash. It publishes `π`, and for each action the index of the action of `T`
that carries the same nullifier, `cv'_i`, `rk'_i`, the dummy output commitment
`cmx'_i` and the signature.

The prover MUST NOT publish `D`'s value balance or binding signature: they would
reveal the total value of the spent notes, which a disclosure does not need.

`D` can never be mined: its nullifiers are already in the nullifier set.

ZIP 311 re-proves Sapling spends against the transaction's anchor "so wallets
can reuse the witness they already keep, and the anonymity set was already
fixed by the original spend". The same reasoning applies here. A wallet that
wants to disclose payments later should keep, at send time, each spent note and
its path to `rt` (the reference wallet stores these as a "disclosure kit").

### Spend authority, re-signed (when the wallet kept `α`)

A wallet that kept the randomizer `α_i` of a spend of `T` (for example from the
PCZT it signed) MAY instead sign the digest with `ask` randomized by `α_i`,
producing a signature that verifies under `T`'s own `rk_i`. `T`'s own proof
already ties `rk_i` to `nf_i`, so no new proof is needed. This reveals nothing
new: `rk_i` is already public.

### Outputs

For each disclosed output action `j`, the prover reveals

```
ock_j = PRF^ock_Orchard(ovk, cv_j, cmx_j, epk_j)
```

computed with the outgoing viewing key the output was created with (external
for payments, internal for change). The verifier decrypts `out_ciphertext_j`
with `ock_j`, obtains `(pk_d, esk)`, decrypts the note plaintext, and checks
the note commitment against `cmx_j` and `esk` against the note's `rseed`
(ZIP 212). This opens output `j` and no other. An output created with
`ovk = ⊥` cannot be disclosed.

### Digest

```
digest = BLAKE2b-256(personalization = "ZIP311Signed" || I2LEOSP_32(coinType),
                     data = unsignedEncoding)
```

`coinType` is the SLIP-44 index, not hardened: 133 on mainnet, 1 on testnet,
as in ZIP 311. `unsignedEncoding` is the unsigned part of the encoding below. It
covers every public input of `π` except the anchor and the nullifiers, which are
fixed by `txid` and the action indices. It does not cover the bytes of `π`: any
valid proof for the same public inputs attests the same statement, and a
re-proved bundle's proof is only available after the bundle is built, so
excluding it lets the digest be computed once, before signing.

### Encoding (version 1)

All integers little-endian; `CompactSize` as in Bitcoin.

```
unsigned part:
  version            u8 = 1
  network            u8           0 = mainnet, 1 = testnet
  txid               32 bytes     as serialized (not display order)
  height             u32
  msg                CompactSize length || bytes          (≤ 1024)
  reproved           CompactSize n || n × (actionIndex u32 || cv' 32 || rk' 32 || cmx' 32)
  resigned           CompactSize n || n × (actionIndex u32)
  outputs            CompactSize n || n × (actionIndex u32 || ock 32)
authorization:
  n × spendAuthSig   64 bytes each, in reproved order
  π                  CompactSize length || bytes          (present iff reproved n > 0)
  n × spendAuthSig   64 bytes each, in resigned order
attachment (not covered by the digest):
  tx                 CompactSize length || raw transaction   (0 = absent)
```

The attached transaction lets a verifier avoid asking a server for `T` by
txid; the verifier MUST check that it hashes to `txid` and that `txid` is mined
at `height`.

Text form: Bech32m of the bytes under the human-readable part `zpd` (mainnet) or
`zpdtest` (testnet), with the length limit lifted as in ZIP 316. A typical
single-spend, single-output disclosure is about 5.3 KB of bytes and 8,500
characters of text (about 23,000 with the transaction attached); the proof is
most of it.

Link form: `https://<verifier app>/v#<text>`. The fragment never leaves the
browser, so the hosting server does not see the disclosure.

### Challenge convention

`msg` may be any bytes. Verifiers SHOULD issue a fresh challenge and check
`msg` against it; otherwise a disclosure made for one verifier can be replayed
to another. This implementation uses

```
quittance:v1|<verifier>|<context>|<nonce: 32 hex>|<expiry: unix seconds>
```

where `verifier` and `context` contain no `|` or newline.

### Verification

Given a disclosure `d`, the chain's copy of `T` with its mined height, and an
optional expected challenge:

1. `d.network` matches the verifier's network.
2. `T` hashes to `d.txid`.
3. `T` is mined at `d.height`.
4. `T` has an Ironwood bundle `B`. If it has only an Orchard bundle, refuse (see "Orchard").
5. At least one spend is present. All action indices are in range and none repeats within the spends or within the outputs.
6. Compute `digest`.
7. If re-proved spends are present: for each, build the instance
   `(rt = B.anchor, cv', nf = B.actions[actionIndex].nf, rk', cmx', enableSpends = 1, enableOutputs = 0, disableCrossAddress = 0)`
   and verify `π` over all instances with the PostNu6_3 verifying key. Then verify each `spendAuthSig` under its `rk'` over `digest`.
8. For each re-signed spend, verify `spendAuthSig` under `B.actions[actionIndex].rk` over `digest`.
9. If an expected challenge was given, `d.msg` must equal it. Otherwise report that the disclosure is not bound to a challenge of the verifier's.
10. Each `ock` must open its output, with the note commitment and `esk` checks above.

Report each step separately: a refusal should say which claim failed.

### Orchard

Verifiers MUST refuse disclosures about Orchard-pool spends. The soundness bug
in the Orchard circuit found in May 2026 (fixed by NU6.2 and NU6.3) means a
proof under that circuit does not establish that a real note was spent, so it
cannot be evidence of spend authority. Orchard funds are withdraw-only since
NU6.3. The reference implementation reports this as the refusal reason.

## Privacy

A disclosure reveals: the txid and height; the disclosed outputs' values,
recipients and memos; that the prover controls the spend of the disclosed
nullifiers; and the challenge.

It does not reveal: the prover's address or any viewing key; the value of the
spent notes (`cv'` is a hiding commitment, and `D`'s value balance is never
published); undisclosed outputs, including change; anything about other
transactions. `rk'` and `cv'` are freshly randomized; the only link to `T` is the
nullifier, which `T` already publishes.

What the verifier's environment learns is outside the disclosure itself: a
verifier that looks `T` up by txid tells the server it queries which
transaction it is interested in. Attaching `T` avoids the lookup by txid.

## Security

* **Replay.** Bind `msg` to a fresh challenge; reject on mismatch.
* **Forgery by the recipient.** The recipient can open the payment, but it does
  not know a spent note behind any of `T`'s nullifiers, so it cannot produce a
  re-proof or a re-signature.
* **Substituted keys.** Replacing `rk'` invalidates `π`, whose instance includes `rk'`.
* **Multi-party transactions.** The statement is "the prover authorized a spend
  in the transaction that paid `A`". In a transaction built by several parties
  the prover may not be the party that created the disclosed output.
* **Resource limits.** Implementations SHOULD cap `msg` (1024 bytes), action
  counts (64), proof size and attachment size before allocating.

## Reference implementation and test vectors

`crates/zip311` (Rust): `prove_reproved`, `verify`, encoding, and tests
including every refusal above. Test vectors from real transactions are in
`test-vectors/`.
