import type { Metadata } from "next";
import { DOCS, REPO } from "@/lib/links";
import styles from "../page-shell.module.css";
import docs from "./docs.module.css";

export const metadata: Metadata = { title: "Docs" };

const API = process.env.NEXT_PUBLIC_QUITTANCE_API ?? "https://api.example";

const SECTIONS = [
  { id: "cli", label: "Command line" },
  { id: "browser", label: "In the browser" },
  { id: "api", label: "HTTP API" },
  { id: "agents", label: "For agents" },
];

export default function DocsPage() {
  return (
    <main className={styles.page}>
      <header className={styles.intro}>
        <p className={styles.eyebrow}>Docs</p>
        <h1 className={styles.title}>Use Quittance</h1>
        <p className={styles.lede}>
          Four ways in: the command line for the person who paid, and the browser, an HTTP API or an agent for whoever asks for proof. The format is specified in the{" "}
          <a href={DOCS.addendum}>ZIP 311 Ironwood addendum</a>; who learns what is in the <a href={DOCS.threatModel}>threat model</a>.
        </p>
      </header>

      <div className={docs.body}>
        <nav className={docs.toc} aria-label="On this page">
          {SECTIONS.map((s) => (
            <a key={s.id} href={`#${s.id}`}>
              {s.label}
            </a>
          ))}
        </nav>

        <div className={docs.sections}>
          <section className={docs.section} id="cli" aria-labelledby="cli-title">
            <h2 id="cli-title" className={styles.h2}>
              Prove a payment (command line)
            </h2>
            <p className={styles.note}>The wallet only uses the Ironwood pool and saves a disclosure kit every time it sends, so any past payment can be proven later.</p>
            <pre className={docs.pre}>{`cargo install --git ${REPO} quittance-cli

quittance wallet init --network main --dir ./w --mnemonic-file seed.txt --birthday 3530000
quittance wallet sync --dir ./w
quittance wallet send --dir ./w --to u1… --zat 25000 --memo "Invoice 2026-118"

quittance prove --dir ./w --txid <txid> --challenge "quittance:v1|Acme|ticket 8812|<nonce>|<expiry>" --embed-tx
quittance verify zpd1… --challenge "quittance:v1|…"`}</pre>
          </section>

          <section className={docs.section} id="browser" aria-labelledby="browser-title">
            <h2 id="browser-title" className={styles.h2}>
              Verify in the browser
            </h2>
            <p className={styles.note}>
              The verifier is the same Rust code compiled to WebAssembly. Load it once; the verifying key is derived from the circuit on first use. Fetch the transaction
              yourself, or let the receipt carry it.
            </p>
            <pre className={docs.pre}>{`import init, { verify, inspect } from "/wasm/quittance.js";
await init();

const claim = JSON.parse(inspect(receipt));            // network, txid, height, challenge
const { raw, height } = await (await fetch(\`${API}/v1/tx/\${claim.network}/\${claim.txid}\`)).json();
const verdict = JSON.parse(verify(receipt, raw, height, myChallenge, claim.network, Date.now() / 1000));
verdict.valid;              // true or false
verdict.revealed.payments;  // [{ value_zec, recipient, memo }]
verdict.checks;             // every check, with the reason when one fails`}</pre>
          </section>

          <section className={docs.section} id="api" aria-labelledby="api-title">
            <h2 id="api-title" className={styles.h2}>
              HTTP API
            </h2>
            <dl className={docs.endpoints}>
              <div>
                <dt>
                  <code>POST /v1/verify</code>
                </dt>
                <dd>{"{ disclosure, challenge? }"} → verdict</dd>
              </div>
              <div>
                <dt>
                  <code>POST /v1/challenges</code>
                </dt>
                <dd>{"{ verifier, context, ttl_seconds?, network? }"} → challenge with an answer link</dd>
              </div>
              <div>
                <dt>
                  <code>GET /v1/challenges/:id</code>
                </dt>
                <dd>status, and the verdict once answered</dd>
              </div>
              <div>
                <dt>
                  <code>POST /v1/challenges/:id/answer</code>
                </dt>
                <dd>{"{ disclosure }"} → verified against that challenge</dd>
              </div>
              <div>
                <dt>
                  <code>GET /v1/tx/:network/:txid</code>
                </dt>
                <dd>raw transaction and mined height</dd>
              </div>
              <div>
                <dt>
                  <code>GET /v1/block/:network/:height</code>
                </dt>
                <dd>txids in a block, for checking an attached transaction</dd>
              </div>
            </dl>
            <pre className={docs.pre}>{`curl -s ${API}/v1/verify -H 'content-type: application/json' \\
  -d '{"disclosure":"zpd1…","challenge":"quittance:v1|…"}'`}</pre>
          </section>

          <section className={docs.section} id="agents" aria-labelledby="agents-title">
            <h2 id="agents-title" className={styles.h2}>
              For agents (MCP)
            </h2>
            <p className={styles.note}>
              A support agent can ask a customer for proof and check it without ever holding their viewing key. Tools: <code>create_payment_challenge</code>,{" "}
              <code>check_payment_challenge</code>, <code>verify_payment_disclosure</code>.
            </p>
            <pre className={docs.pre}>{`{
  "mcpServers": {
    "quittance": {
      "command": "node",
      "args": ["quittance/mcp/dist/index.js"],
      "env": { "QUITTANCE_API": "${API}" }
    }
  }
}`}</pre>
          </section>
        </div>
      </div>
    </main>
  );
}
