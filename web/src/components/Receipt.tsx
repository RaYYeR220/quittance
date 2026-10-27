import type { CSSProperties } from "react";
import { explorerTx } from "@/lib/links";
import type { ChainLookup } from "@/lib/verifier";
import type { Check, Memo, Verdict } from "@/lib/types";
import styles from "./Receipt.module.css";

function shorten(ua: string): string {
  return ua.length > 24 ? `${ua.slice(0, 10)}…${ua.slice(-6)}` : ua;
}

function memoText(m: Memo): string | null {
  if (m.kind === "text") return m.value;
  if (m.kind === "bytes") return `0x${m.value}`;
  return null;
}

const MARK: Record<Check["status"], string> = { pass: "✓", fail: "✕", warn: "!", skip: "–" };
const STATUS: Record<Check["status"], string> = { pass: "passed", fail: "failed", warn: "warning", skip: "skipped" };

/** Lengths of the engraved voids, so the withheld lines don't look stamped from one die. */
const VOIDS = ["82%", "46%", "64%", "92%", "58%", "74%"];

type State = "proven" | "warn" | "refused";

const STAMP: Record<State, string> = {
  proven: "Proven",
  warn: "Proven, not bound to you",
  refused: "Refused",
};

/** What a verdict proves, what it leaves out, and every check behind it. */
export function Receipt({ verdict, lookup }: { verdict: Verdict; lookup?: ChainLookup }) {
  const firstFail = verdict.checks.find((c) => c.status === "fail");
  const unbound = verdict.checks.find((c) => c.id === "challenge" && c.status === "warn");
  const unit = verdict.network === "main" ? "ZEC" : "TAZ";
  const state: State = verdict.valid ? (unbound ? "warn" : "proven") : "refused";

  return (
    <article className={styles.receipt} data-verdict={state} aria-live="polite">
      <div className={styles.sheet}>
        <header className={styles.head}>
          <span className={styles.stamp}>
            <span aria-hidden="true">{state === "refused" ? "✕" : state === "warn" ? "!" : "✓"}</span> {STAMP[state]}
          </span>
          <span className={styles.net}>{verdict.network === "main" ? "Zcash mainnet" : "Zcash testnet"}</span>
          <span className={styles.serial} aria-hidden="true">
            Nº {verdict.height}
          </span>
        </header>

        {verdict.valid && verdict.revealed ? (
          <div className={styles.halves}>
            <section className={styles.proven} aria-label="What this receipt proves">
              <h3 className={styles.halfTitle}>Proven</h3>
              <ul className={styles.lines}>
                {verdict.revealed.payments.map((p) => (
                  <PaymentLines key={p.action_index} amount={`${p.value_zec} ${unit}`} recipient={p.recipient} memo={memoText(p.memo)} />
                ))}
                <li>
                  <span className={styles.tick} aria-hidden="true">
                    ✓
                  </span>
                  <span className={styles.key}>Mined at height</span>
                  <span className={`${styles.value} ${styles.mono}`}>
                    {verdict.height.toLocaleString("en-US")}{" "}
                    <a href={explorerTx(verdict.network, verdict.txid)} target="_blank" rel="noopener noreferrer" className={styles.txLink}>
                      view transaction ↗
                    </a>
                  </span>
                </li>
                <li className={styles.wide}>
                  <span className={styles.tick} aria-hidden="true">
                    ✓
                  </span>
                  <span className={styles.value}>Sent by the holder of the spending key behind this payment</span>
                </li>
                <li>
                  <span className={styles.tick} aria-hidden="true">
                    {unbound ? "!" : "✓"}
                  </span>
                  <span className={styles.key}>Signed for</span>
                  <span className={`${styles.value} ${styles.who}`}>
                    {verdict.challenge ? `${verdict.challenge.verifier} · ${verdict.challenge.context}` : (verdict.challenge_text ?? "an empty challenge")}
                  </span>
                </li>
              </ul>
              {unbound && (
                <p className={styles.warn}>
                  You didn&apos;t give the challenge you issued, so this could be a receipt made for someone else. Paste your challenge to rule that out.
                </p>
              )}
            </section>

            <section className={styles.withheld} aria-label="What this receipt does not reveal">
              <h3 className={styles.halfTitle}>Not revealed</h3>
              <ul className={styles.voids}>
                {verdict.not_revealed.map((n, i) => (
                  <li key={n}>
                    <p>{n}</p>
                    <span className={styles.void} style={{ width: VOIDS[i % VOIDS.length] } as CSSProperties} aria-hidden="true" />
                  </li>
                ))}
              </ul>
            </section>
          </div>
        ) : (
          <section className={styles.refusal} aria-label="Why it was refused">
            <p className={styles.reasonTitle}>{firstFail?.label ?? "Not verified"}</p>
            {firstFail?.detail && <p className={styles.reasonDetail}>{firstFail.detail}</p>}
          </section>
        )}

        <section className={styles.register} aria-label="Checks">
          <h3 className={styles.registerTitle}>Checks</h3>
          <ol className={styles.checks}>
            {verdict.checks.map((c) => (
              <li key={c.id} data-status={c.status}>
                <span className={styles.mark} aria-hidden="true">
                  {MARK[c.status]}
                </span>
                <span className="visually-hidden">{STATUS[c.status]}: </span>
                <span className={styles.label}>{c.label}</span>
                <span className={styles.detail}>{c.detail}</span>
              </li>
            ))}
          </ol>
        </section>

        {lookup && (
          <footer className={styles.foot}>
            {lookup.kind === "block"
              ? `Checked in this browser. Our server was asked only for the transactions in block ${lookup.height.toLocaleString("en-US")}.`
              : "Checked in this browser. Our server was asked for this transaction by its id."}
          </footer>
        )}
      </div>
    </article>
  );
}

function PaymentLines({ amount, recipient, memo }: { amount: string; recipient: string; memo: string | null }) {
  return (
    <>
      <li>
        <span className={styles.tick} aria-hidden="true">
          ✓
        </span>
        <span className={styles.key}>Paid</span>
        <span className={`${styles.value} ${styles.amount}`}>{amount}</span>
      </li>
      <li>
        <span className={styles.tick} aria-hidden="true">
          ✓
        </span>
        <span className={styles.key}>To</span>
        <span className={`${styles.value} ${styles.mono}`} title={recipient}>
          {shorten(recipient)}
        </span>
      </li>
      <li>
        <span className={styles.tick} aria-hidden="true">
          ✓
        </span>
        <span className={styles.key}>Memo</span>
        <span className={`${styles.value} ${styles.memo}`}>{memo ? `“${memo}”` : <span className={styles.dim}>none</span>}</span>
      </li>
    </>
  );
}
