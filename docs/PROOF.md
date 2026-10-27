# Proof

Every claim Quittance makes, with a way to check it yourself. Transaction
links open the mainnet explorer.

## Check it in five minutes

1. **Watch a real wallet answer your challenge.** Open the [live demo](https://quittance-zec.vercel.app/demo), type who
   you are, press *Ask for a receipt*. A mainnet wallet makes a fresh receipt for
   your challenge and your browser checks it against the chain.
2. **Try to break it.** On the same page, each of the five buttons sends a
   forgery: a replay, a changed output key, a changed proof, the recipient
   claiming the payment, a receipt pointed at an Orchard transaction. Each is
   refused with the check that failed.
3. **Replay the mainnet receipts offline.** `cargo test -p zip311 --test mainnet`
   verifies the three receipts below against the raw transactions as the chain
   served them, and checks that tampered copies fail.
4. **Read the construction.** [`zip-0311-ironwood.md`](zip-0311-ironwood.md) is
   the specification; [`THREAT-MODEL.md`](THREAT-MODEL.md) says who learns what.

## Mainnet payments

The demo wallet made three Ironwood payments to the demo recipient on
2026-10-27. Each was disclosed by its sender, verified with the right challenge
(valid) and with someone else's (refused).

| Payment | Amount | Height | Transaction |
|---|---|---|---|
| Invoice 2026-118 · Halcyon Studio | 0.0002 ZEC | 3,532,223 | [06a8b50f…2f45b6](https://mainnet.zcashexplorer.app/transactions/06a8b50f6db826a2533eecfd04fb98beb8df2948494c219efab22d60832f45b6) |
| Grant payout · milestone 2 of 4 | 0.0003 ZEC | 3,532,228 | [78df1f3d…d06a39](https://mainnet.zcashexplorer.app/transactions/78df1f3df4259620da37d5c7c9effcf29b519aade405f8d34850fd3c82d06a39) |
| Refund · order 5521 | 0.00025 ZEC | 3,532,234 | [580e9621…d17d51c](https://mainnet.zcashexplorer.app/transactions/580e9621f7fc9625d66c6b8ef844d4aa0d1e0736773ffbf3a1d2baf66d17d51c) |

The receipts, their challenges and the raw transactions are in
[`test-vectors/mainnet/`](../test-vectors/mainnet/). The second payment spends
the change of the first, so a disclosure of a change-funded payment is covered
too.

How the demo wallet was funded: 0.00184 ZEC arrived from an exchange bridge at a
transparent address of a separate funding wallet
([de37f475…e99f10](https://mainnet.zcashexplorer.app/transactions/de37f475f7e75644161353b9f0862e2e42f669974ef33724781d829996e99f10)),
which shielded it into Ironwood
([b4a966bb…84fae4](https://mainnet.zcashexplorer.app/transactions/b4a966bb97bb7d92db4cd23614457cc29a7b678ba6cf21a51f22178db784fae4))
and paid the demo wallet shielded
([1885c27e…c67db3](https://mainnet.zcashexplorer.app/transactions/1885c27ef69ed241aeb91bb41f13649cc394ee0d2d26f534a2619957f2c67db3)).
The demo wallet itself has never touched a transparent address.

The Orchard refusal uses a real Orchard-pool payment,
[42f33910…833fa](https://mainnet.zcashexplorer.app/transactions/42f3391082a55a6bf36e91ca8d6069dbcb2d88d27b5b960e24e3f119f99833fa)
at height 3,428,100.

## Claims

| Claim | Evidence | How to check |
|---|---|---|
| A receipt proves the payment's amount, recipient and memo, and that the prover made it | three mainnet receipts verify | `cargo test -p zip311 --test mainnet`; live demo |
| It is bound to the verifier's challenge | a receipt checked against another challenge is refused | `a_receipt_checked_against_another_challenge_is_refused`; demo "Replay" |
| The recipient cannot claim to be the sender | their wallet refuses: it spent nothing in the transaction | `the_recipient_cannot_disclose_as_the_sender`; demo "Let the recipient claim it" |
| A changed key, proof or signature is refused | each fails its own check | `tampering_with_a_real_receipt_is_refused` and 9 synthetic tests |
| Orchard-pool payments are refused | pool check | `orchard_pool_transactions_are_refused_with_the_reason`; demo "Orchard" |
| The re-proof doesn't reveal the spent value | the bundle's value balance and binding signature are never encoded | `format.rs` (no such field); [spec](zip-0311-ironwood.md#privacy) |
| The browser check tells our server at most a block height | receipts with the transaction attached are checked with `/v1/block` only | `web/src/lib/verifier.ts` |
| The demo wallet only uses Ironwood | it refuses transparent, TEX, Sapling and Orchard recipients and inputs | `crates/quittance-wallet/src/recipient.rs`, `send.rs` tests |

## Numbers

- A receipt's spend proof: 4,992 bytes. A receipt as text: about 8,500
  characters, or about 23,000 with the transaction attached.
- Making a receipt: 0.6 s on a laptop, 3.9 s on the demo server (2 shared vCPUs).
- Checking it: 6 ms native. In a browser the first check also builds the
  verifying key from the circuit, which takes 10–20 s; later checks are instant.
- Tests: 54 Rust tests (zip311 37, including 6 against mainnet receipts; wallet
  12; CLI 2; server 3) and 4 for the MCP server.

## Not claimed

- Sapling disclosures (ZIP 311's original case) are not implemented.
- Re-signed disclosures (signing with a kept randomizer) verify, but the
  reference wallet only makes re-proved ones.
- No Tor support in the CLI or the web app.
- No third-party audit.
