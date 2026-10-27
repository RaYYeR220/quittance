import { test } from "node:test";
import assert from "node:assert/strict";
import { QuittanceApi, describeVerdict, type Verdict } from "./api.js";

const valid: Verdict = {
  valid: true,
  network: "main",
  txid: "ab".repeat(32),
  height: 3_540_000,
  challenge_text: "quittance:v1|Acme|ticket 1|00|1",
  checks: [{ id: "spend-proof", label: "Sender: spend re-proven", status: "pass", detail: "Halo2 proof over 1 action(s)" }],
  revealed: {
    payments: [{ action_index: 1, value_zat: 30000, value_zec: "0.0003", recipient: "u1xyz", memo: { kind: "text", value: "invoice 42" } }],
    spends: [{ action_index: 0, nullifier: "00", mode: "reproved" }],
  },
  not_revealed: ["the sender's address or viewing key"],
};

test("a valid verdict reads as one actionable sentence", () => {
  const text = describeVerdict(valid);
  assert.match(text, /^VALID: .* paid 0\.0003 ZEC to u1xyz with memo "invoice 42"/);
  assert.match(text, /Not revealed: the sender's address/);
});

test("a refusal leads with the first failed check", () => {
  const refused: Verdict = {
    ...valid,
    valid: false,
    revealed: null,
    checks: [
      { id: "network", label: "Network", status: "pass", detail: "mainnet" },
      { id: "challenge", label: "Bound to your challenge", status: "fail", detail: "signed over a different challenge" },
    ],
  };
  assert.match(describeVerdict(refused), /^REFUSED: Bound to your challenge: signed over a different challenge\./);
});

test("API errors surface the server's reason", async () => {
  const fake = (async () => new Response(JSON.stringify({ error: "not a payment disclosure" }), { status: 400 })) as typeof fetch;
  const api = new QuittanceApi("https://x.test", fake);
  await assert.rejects(api.verify("zpd1bad"), /400: not a payment disclosure/);
});
