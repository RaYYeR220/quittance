# Threat model

Quittance lets the sender of a shielded Zcash payment prove that payment to
one verifier. This document lists who can learn what, at every step, and what
Quittance does and does not defend against. If a claim here is wrong, it is a
bug: please open an issue.

## Parties

| Party | Wants |
|---|---|
| **Prover** (the sender) | to prove one payment and reveal nothing else about their wallet |
| **Verifier** (exchange, merchant, committee, auditor) | to be sure the payment happened, to whom, for how much, and that the person in front of them made it |
| **Recipient** | not to have the payment pinned on them, and not to be exposed beyond the disclosed output |
| **Quittance server operator** (us, or whoever self-hosts) | runs the verify API, the challenge store and the demo |
| **lightwalletd operator** | serves chain data to wallets and to our server |
| **Anyone who obtains a disclosure** | learns what it discloses |

## What a disclosure reveals

A disclosure is a bearer document. Whoever holds it can read:

- the txid and the height it was mined at;
- for each disclosed output: the value, the recipient address (as a unified
  address with a single Ironwood receiver) and the memo;
- the nullifiers of the disclosed spends, which the transaction already
  publishes, and the fact that the prover controls those spends;
- the challenge it was signed over.

It does not reveal, to anyone:

- the prover's address, diversifier or any viewing key;
- the value of the notes the prover spent (`cv'` is a hiding commitment and
  the re-proof's value balance is never published);
- the change or any undisclosed output;
- any other transaction of the prover or the recipient.

The re-proof uses fresh randomness (`α`, `rcv`), so `rk'` and `cv'` cannot be
linked to the original spend's `rk` and `cv`. The only link to the transaction
is the nullifier, which is already public.

**Consequence:** share a disclosure only with the verifier who asked. A
disclosure forwarded elsewhere still verifies, but the "bound to your
challenge" check fails for anyone but the verifier who issued the challenge.

## Step by step

### Proving (CLI wallet, `quittance prove`)

| Who | Learns |
|---|---|
| lightwalletd | what any light wallet reveals: the wallet's IP, the block ranges it syncs, and the txids it fetches in full. Quittance's wallet adds nothing to this |
| Quittance server | nothing, unless the prover answers a challenge through it (below) |

The disclosure kit (spent notes and their Merkle paths at the transaction's
anchor) stays in the wallet directory. It is as sensitive as the wallet's own
database. The mnemonic is never logged or sent anywhere.

Payments made with the wallet only spend from and send to the Ironwood pool. It
refuses transparent recipients, TEX addresses, and any payment that would touch
the transparent, Sapling or Orchard pools, so a payment has no transparent leg
to leak. The one exception is funding: if you ask for a transparent address
(for an exchange or bridge that can only pay one), `wallet shield` moves what
arrives there into Ironwood. That deposit and the shielding are public, as any
transparent receipt is; nothing else in the wallet touches transparent funds.

### Verifying in the browser (`/verify`)

- The disclosure travels in the URL **fragment** (`/v#zpd1…`). Browsers do not
  send fragments to the server, so the web host never sees it.
- The WebAssembly verifier runs on the verifier's machine.
- To check the transaction is mined, the page needs chain data:
  - if the prover attached the transaction, the page asks our API only for the
    txids of the block at the claimed height (`/v1/block/…`): the server learns
    a height, not which transaction;
  - otherwise the page asks for the transaction by txid (`/v1/tx/…`): the
    server learns that someone is interested in that transaction, and from
    which IP.
- Self-hosting the API (one binary) removes us from the picture entirely.

### Verifying through the API (`POST /v1/verify`, the MCP server)

The server sees the disclosure, by design. Use the browser path or self-host
if that matters. The server does not log disclosures; it keeps a bounded
in-memory transaction cache keyed by txid.

### Ask for proof (`/ask`, `/v1/challenges`)

The server stores the challenge (verifier name, context, nonce, expiry) and,
once answered, the disclosure and the verdict, so the verifier can read them
later. Tickets are dropped seven days after they expire. The verifier name and
context are whatever the verifier typed: do not put personal data there.

### Live demo (`/demo`)

The demo wallet's keys live on our server and hold a few cents of real ZEC.
The payments it discloses were made for the demo. Nothing a visitor types is
stored beyond the request.

## Attacks considered

| Attack | Outcome |
|---|---|
| Replaying a disclosure made for someone else | the challenge check fails for any verifier but the one who issued it |
| Rewriting the challenge text | the spend authorization signature no longer verifies |
| The recipient claiming to be the sender | impossible: the recipient knows no note behind the transaction's nullifiers, so it can produce neither a re-proof nor a re-signature |
| Swapping in another key (`rk'`) and re-signing | the Halo2 proof was made for the original `rk'`, so it fails |
| Pointing a spend or output at another action | proof or decryption fails |
| Tampering with the output key, proof or signature | refused, with the failing check named |
| Claiming a transaction that is not mined, or at another height | refused |
| Using a disclosure on the other network | refused; the coin type is also in the digest |
| An Orchard-pool spend | refused: the Orchard circuit's 2026 soundness bug means its proofs are not evidence |
| Oversized input | lengths are capped before allocation (challenge 1 KiB, 64 actions, 128 KiB proof, 2 MiB transaction); request bodies are capped at 4 MiB |

## Out of scope and honest limits

- **Network-level anonymity.** Neither the CLI nor the web app routes traffic
  over Tor. Use a system-wide Tor or VPN if your IP matters.
- **Multi-party transactions.** A disclosure proves the prover authorized a
  spend in the transaction that paid the recipient. If several parties built
  the transaction together, the prover need not be the one who funded the
  disclosed output.
- **Re-signed mode** needs the spend's randomizer `α`, which a wallet only has
  if it kept it when signing (for example from a PCZT). The reference wallet
  produces re-proved disclosures.
- **Kits must be captured.** A re-proof needs each spent note's Merkle path at
  the transaction's anchor. The reference wallet saves it at send time; for an
  older transaction the path has to be rebuilt from the chain.
- **Outputs created with `ovk = ⊥`** cannot be disclosed.
- **Sapling** disclosures (ZIP 311's original case) are not implemented here.
- **Who holds the disclosure** is not controlled: it is a bearer document.
