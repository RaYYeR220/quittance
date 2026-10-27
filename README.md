# Quittance

**Prove one shielded Zcash payment. Reveal nothing else.**

When an exchange, a merchant, a grant committee or an auditor asks you to prove
a shielded payment, today you have two bad options: hand over a viewing key,
which exposes your whole history, or hand over a txid, which proves nothing
because anyone can point at a transaction.

Quittance gives you a third: a **payment disclosure**. It shows one payment
(the amount, the recipient's address, the memo) and proves that *you* made it,
signed over the verifier's own challenge so it can't be replayed. It is checked
against the chain, in the verifier's browser. Your viewing key never leaves
your wallet.

It implements [ZIP 311](https://zips.z.cash/zip-0311) (Zcash Payment
Disclosures) for the **Ironwood** pool, which holds nearly all shielded ZEC since
NU6.3. ZIP 311 specifies Sapling only and leaves Orchard as future work; the
Ironwood construction, an encoding and a challenge convention are specified in
[`docs/zip-0311-ironwood.md`](docs/zip-0311-ironwood.md).

> Live: https://quittance-zec.vercel.app · API: https://34-78-220-3.sslip.io · Demo video: https://youtu.be/QlgTtyQ_4Tw · [Check our claims in five minutes](docs/PROOF.md)

## How it works

A disclosure is itself an Ironwood bundle that can never be mined.

1. **Who paid.** The prover re-proves the spend of the note the payment
   consumed, under the payment's own anchor, with outputs disabled so the
   bundle moves no value. The re-proof carries the same nullifier the payment
   published, with fresh randomness everywhere else. Its spend authorization
   signature covers the disclosure digest instead of a sighash, binding it to
   the verifier's challenge. Same Halo2 circuit, same RedPallas signatures, same
   verifying key as consensus: no new circuit, no trusted setup.
2. **Whom, how much, what memo.** For each disclosed output the prover reveals
   its outgoing cipher key, which opens that output and no other.
3. **Nothing else.** The re-proof's value balance and binding signature are
   never published, so the value of the spent notes stays hidden; so does the
   change, the prover's address, and every other transaction.

```
 sender's wallet                                   verifier
 ───────────────                                   ────────
 payment T (Ironwood) ──── mined ────────────────▶ chain
 kit: spent note + path at T's anchor
        │
        │  challenge C ◀─────────────────────────── "prove deposit #8812"
        ▼
 re-prove spend(s) under T.anchor  (outputs off)
 sign digest(txid, C, outputs, …) with ask·α'
 reveal ock for the paid output
        │
        └──── zpd1… (≈8.5k chars) ───────────────▶ verify in browser (WASM)
                                                   · T mined at h, hashes to txid
                                                   · Halo2 proof vs T.anchor, T.nf
                                                   · signature over digest(C)
                                                   · ock opens the output
                                                   → "paid 0.0125 ZEC to u1…, memo …"
```

Every check is reported separately, so a refusal says exactly which claim
failed: a different challenge, a tampered key, a proof that doesn't match the
transaction, an Orchard-pool spend (refused: that circuit's 2026 soundness bug
means its proofs are not evidence), a transaction that isn't mined.

## What's in the repository

| Path | What |
|---|---|
| `crates/zip311` | the library: disclosure format and encoding, `prove_reproved`, `verify`, test suite |
| `crates/quittance-wallet` | a light-wallet engine on `zcash_client_sqlite` that only pays from and to Ironwood, saves a disclosure kit at send time, and can shield funds that arrive at a transparent address |
| `crates/quittance-cli` | the `quittance` command: wallet, `prove`, `verify`, `challenge` |
| `crates/quittance-server` | HTTP API: verify, transaction and block lookup, "ask for proof" challenges, live demo |
| `crates/quittance-wasm` | the verifier compiled to WebAssembly for the browser |
| `web/` | the web app: verify, ask for proof, live demo, docs |
| `mcp/` | an MCP server so a support agent can ask for and check proofs |
| `docs/` | the ZIP 311 Ironwood addendum and the threat model |

## Quick start

```sh
# verify a disclosure from the command line
cargo run -p quittance-cli --release -- verify zpd1... --challenge "quittance:v1|..."

# make one: create a wallet, pay someone, then prove it
quittance wallet init --network main --dir ./w --mnemonic-file seed.txt --birthday 3530000
quittance wallet sync --dir ./w
quittance wallet send --dir ./w --to u1... --zat 1250000 --memo "Invoice 2026-118"
quittance prove --dir ./w --txid <txid> --challenge "quittance:v1|Acme|ticket 8812|<nonce>|<expiry>"
```

Run the API locally:

```sh
QUITTANCE_BIND=127.0.0.1:8080 cargo run -p quittance-server --release
curl -s localhost:8080/v1/verify -H 'content-type: application/json' \
  -d '{"disclosure":"zpd1...","challenge":"quittance:v1|..."}'
```

Build the browser verifier (needs Docker; libsecp256k1 needs clang for wasm32):

```sh
sh scripts/build-wasm.sh web/public/wasm
```

### Configuration

| Variable | Default | |
|---|---|---|
| `QUITTANCE_BIND` | `0.0.0.0:8080` | listen address |
| `QUITTANCE_LWD_MAIN` | `https://zec.rocks:443,https://eu.zec.rocks:443,https://na.zec.rocks:443` | mainnet lightwalletd, tried in order |
| `QUITTANCE_LWD_TEST` | `https://testnet.zec.rocks:443` | testnet lightwalletd |
| `QUITTANCE_APP_ORIGIN` | `http://localhost:3000` | web app origin, for answer links |
| `QUITTANCE_DATA` | unset | directory for challenge tickets and the gallery |

## Privacy

See [`docs/THREAT-MODEL.md`](docs/THREAT-MODEL.md) for who learns what at
every step. In short: a disclosure reveals the disclosed outputs and that you
made the payment, nothing more; the web app never sees the disclosure (it
travels in the URL fragment); verifying in the browser tells our API at most
which block you're checking.

## Limits

- Ironwood only. Orchard-pool spends are refused on purpose; Sapling (ZIP 311's
  original case) isn't implemented.
- To re-prove a spend the wallet needs the spent note's Merkle path at the
  payment's anchor. The reference wallet saves it when it sends; other wallets
  would need to do the same, or rebuild the path from the chain.
- A disclosure is a bearer document: anyone you forward it to can read it.
- No Tor support in the CLI or the web app.

## License

MIT
