"use client";

import { useEffect, useMemo, useState } from "react";
import { Receipt } from "@/components/Receipt";
import { VerifierStatus } from "@/components/VerifierStatus";
import { api, type DemoInfo, type DemoPayment } from "@/lib/api";
import { tamper, verifyHere, warmUp, type Result } from "@/lib/verifier";
import styles from "../page-shell.module.css";
import demo from "./demo.module.css";

function nonce(): string {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}

function challengeFor(who: string, what: string): string {
  const clean = (s: string) => s.replace(/[|\n]/g, " ").trim() || "anonymous";
  return `quittance:v1|${clean(who)}|${clean(what)}|${nonce()}|${Math.floor(Date.now() / 1000) + 3600}`;
}

type Attempt = { id: string; title: string; how: string };

const ATTEMPTS: Attempt[] = [
  { id: "replay", title: "Replay it to someone else", how: "The same receipt, checked by a verifier who issued a different challenge." },
  { id: "output-key", title: "Change one byte of the output key", how: "The key that opens the payment, altered after signing." },
  { id: "proof", title: "Change one byte of the proof", how: "The re-proof of the spend, altered." },
  { id: "recipient", title: "Let the recipient claim it", how: "The wallet that received the payment tries to disclose it as its sender." },
  { id: "orchard", title: "Point it at an Orchard transaction", how: "The receipt retargeted at a real payment from the Orchard pool." },
];

type Outcome = { refused: boolean; label: string; detail: string };

export default function DemoPage() {
  const [info, setInfo] = useState<DemoInfo | null>(null);
  const [infoError, setInfoError] = useState<string | null>(null);
  const [pick, setPick] = useState(0);
  const [who, setWho] = useState("Acme Exchange");
  const [what, setWhat] = useState("deposit dispute 8812");
  const [challenge, setChallenge] = useState("");
  const [busy, setBusy] = useState(false);
  const [receipt, setReceipt] = useState<{ text: string; payment: DemoPayment; proveMs: number; verifyMs: number; result: Result } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [outcomes, setOutcomes] = useState<Record<string, Outcome | "running">>({});

  useEffect(() => {
    warmUp();
    api.demo().then(setInfo, (e: Error) => setInfoError(e.message));
  }, []);

  const payment = info?.payments[pick];
  const link = useMemo(() => (receipt ? `${window.location.origin}/verify#${receipt.text}` : ""), [receipt]);

  async function ask() {
    const msg = challengeFor(who, what);
    setChallenge(msg);
    setBusy(true);
    setError(null);
    setReceipt(null);
    setOutcomes({});
    try {
      const proof = await api.demoProve(msg, pick);
      const t = performance.now();
      const result = await verifyHere(proof.disclosure, msg);
      setReceipt({ text: proof.disclosure, payment: proof.payment, proveMs: proof.prove_ms, verifyMs: Math.round(performance.now() - t), result });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function attempt(id: string) {
    if (!receipt) return;
    setOutcomes((o) => ({ ...o, [id]: "running" }));
    const fromVerdict = (r: Result): Outcome => {
      const f = r.verdict.checks.find((c) => c.status === "fail");
      return f ? { refused: true, label: f.label, detail: f.detail } : { refused: false, label: "Accepted", detail: "the receipt verified" };
    };
    try {
      let out: Outcome;
      if (id === "replay") {
        out = fromVerdict(await verifyHere(receipt.text, challengeFor("Someone else", "another case")));
      } else if (id === "output-key" || id === "proof") {
        out = fromVerdict(await verifyHere(await tamper(receipt.text, id), challenge));
      } else if (id === "recipient") {
        const r = await api.demoForge(challenge, pick);
        out = { refused: r.refused, label: "Recipient's wallet", detail: r.reason };
      } else {
        const r = await api.demoOrchard(receipt.text);
        out = fromVerdict(await verifyHere(r.disclosure, challenge));
      }
      setOutcomes((o) => ({ ...o, [id]: out }));
    } catch (e) {
      setOutcomes((o) => ({ ...o, [id]: { refused: true, label: "Refused before checking", detail: e instanceof Error ? e.message : String(e) } }));
    }
  }

  return (
    <main className={styles.page}>
      <header className={styles.intro}>
        <p className={styles.eyebrow}>Live demo · Zcash mainnet</p>
        <h1 className={styles.title}>Ask a real wallet for proof</h1>
        <p className={styles.lede}>
          Our demo wallet made a few small payments on mainnet. Ask it about one: it answers your challenge with a fresh receipt, and your browser checks it against the
          chain. Then try to break it.
        </p>
        <VerifierStatus />
      </header>

      {infoError && <p className={styles.error}>The demo wallet is offline right now ({infoError}).</p>}

      {info && (
        <section className={demo.ask} aria-label="Ask for a receipt">
          <fieldset className={demo.payments}>
            <legend>Which payment</legend>
            {info.payments.map((p, i) => (
              <label key={p.txid} className={demo.payment} data-on={i === pick}>
                <input type="radio" name="payment" id={`payment-${i}`} checked={i === pick} onChange={() => setPick(i)} />
                <span className={demo.memo}>{p.memo}</span>
                <span className={demo.meta}>
                  {p.value_zec} ZEC · height {p.height.toLocaleString("en-US")}
                </span>
              </label>
            ))}
          </fieldset>
          <div className={styles.row}>
            <label className={styles.field}>
              <span>Who is asking</span>
              <input id="who" value={who} onChange={(e) => setWho(e.target.value)} maxLength={80} />
            </label>
            <label className={styles.field}>
              <span>What for</span>
              <input id="what" value={what} onChange={(e) => setWhat(e.target.value)} maxLength={80} />
            </label>
          </div>
          <button type="button" className={styles.primary} onClick={() => void ask()} disabled={busy || !payment}>
            {busy ? "Proving and checking…" : "Ask for a receipt"}
          </button>
          {challenge && (
            <p className={demo.challenge}>
              Challenge: <code>{challenge}</code>
            </p>
          )}
        </section>
      )}

      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}

      {receipt && (
        <>
          <p className={demo.timing}>
            Made by the demo wallet in {(receipt.proveMs / 1000).toFixed(1)} s · checked in your browser in {(receipt.verifyMs / 1000).toFixed(1)} s ·{" "}
            {receipt.text.length.toLocaleString("en-US")} characters ·{" "}
            <a href={link} target="_blank" rel="noopener noreferrer">
              open as a link ↗
            </a>
          </p>
          <Receipt verdict={receipt.result.verdict} lookup={receipt.result.lookup} />

          <section className={demo.break} aria-label="Try to break it">
            <h2 className={styles.h2}>Try to break it</h2>
            <ul className={demo.attempts}>
              {ATTEMPTS.map((a) => {
                const o = outcomes[a.id];
                return (
                  <li key={a.id} className={demo.attempt} data-state={o === undefined ? "idle" : o === "running" ? "running" : o.refused ? "refused" : "accepted"}>
                    <div>
                      <p className={demo.attemptTitle}>{a.title}</p>
                      <p className={demo.attemptHow}>{a.how}</p>
                    </div>
                    {o === undefined && (
                      <button type="button" className={styles.ghostButton} onClick={() => void attempt(a.id)}>
                        Try it
                      </button>
                    )}
                    {o === "running" && <span className={demo.result}>checking…</span>}
                    {o && o !== "running" && (
                      <p className={demo.result}>
                        <strong>{o.refused ? "Refused" : "Accepted"}</strong> · {o.label}: {o.detail}
                      </p>
                    )}
                  </li>
                );
              })}
            </ul>
          </section>

          <details className={demo.raw}>
            <summary>The receipt itself</summary>
            <textarea readOnly value={receipt.text} rows={6} id="raw" />
          </details>
        </>
      )}
    </main>
  );
}
