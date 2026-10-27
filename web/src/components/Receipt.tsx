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

function explorer(network: Verdict["network"], txid: string): string {
  return network === "main" ? `https://mainnet.zcashexplorer.app/transactions/${txid}` : `https://testnet.zecblock.com/tx/${txid}`;
}

const MARK: Record<Check["status"], string> = { pass: "✓", fail: "✕", warn: "!", skip: "–" };

/** What a verdict proves, what it leaves out, and every check behind it. */
export function Receipt({ verdict, lookup }: { verdict: Verdict; lookup?: ChainLookup }) {
  const firstFail = verdict.checks.find((c) => c.status === "fail");
  const unbound = verdict.checks.find((c) => c.id === "challenge" && c.status === "warn");
  const unit = verdict.network === "main" ? "ZEC" : "TAZ";
  return (
    <article className={styles.receipt} data-verdict={verdict.valid ? (unbound ? "warn" : "proven") : "refused"} aria-live="polite">
      <header className={styles.head}>
        <span className={styles.stamp}>{verdict.valid ? "Proven" : "Refused"}</span>
        <span className={styles.net}>{verdict.network === "main" ? "Zcash mainnet" : "Zcash testnet"}</span>
      </header>

      {verdict.valid && verdict.revealed ? (
        <section className={styles.block} aria-label="What this receipt proves">
          {verdict.revealed.payments.map((p) => (
            <dl key={p.action_index} className={styles.lines}>
              <div>
                <dt>Paid</dt>
                <dd className={styles.amount}>
                  {p.value_zec} {unit}
                </dd>
              </div>
              <div>
                <dt>To</dt>
                <dd className={styles.mono} title={p.recipient}>
                  {shorten(p.recipient)}
                </dd>
              </div>
              <div>
                <dt>Memo</dt>
                <dd>{memoText(p.memo) ?? <span className={styles.dim}>none</span>}</dd>
              </div>
            </dl>
          ))}
          <dl className={styles.lines}>
            <div>
              <dt>Mined at</dt>
              <dd className={styles.mono}>
                height {verdict.height.toLocaleString("en-US")} ·{" "}
                <a href={explorer(verdict.network, verdict.txid)} target="_blank" rel="noopener noreferrer">
                  view transaction ↗
                </a>
              </dd>
            </div>
            <div>
              <dt>Sent by</dt>
              <dd>the holder of the spending key behind this payment</dd>
            </div>
            <div>
              <dt>Signed for</dt>
              <dd>
                {verdict.challenge ? `${verdict.challenge.verifier} · ${verdict.challenge.context}` : (verdict.challenge_text ?? "an empty challenge")}
              </dd>
            </div>
          </dl>
          {unbound && <p className={styles.warn}>You didn&apos;t give the challenge you issued, so this could be a receipt made for someone else. Paste your challenge to rule that out.</p>}
        </section>
      ) : (
        <section className={styles.block}>
          <p className={styles.reason}>
            <strong>{firstFail?.label ?? "Not verified"}.</strong> {firstFail?.detail}
          </p>
        </section>
      )}

      {verdict.valid && (
        <section className={styles.block} aria-label="What this receipt does not reveal">
          <h3 className={styles.sub}>Not revealed</h3>
          <ul className={styles.void}>
            {verdict.not_revealed.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </section>
      )}

      <section className={styles.block} aria-label="Checks">
        <h3 className={styles.sub}>Checks</h3>
        <ol className={styles.checks}>
          {verdict.checks.map((c) => (
            <li key={c.id} data-status={c.status}>
              <span className={styles.mark} aria-hidden="true">
                {MARK[c.status]}
              </span>
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
    </article>
  );
}
