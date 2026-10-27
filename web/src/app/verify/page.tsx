"use client";

import { useEffect, useState } from "react";
import { Receipt } from "@/components/Receipt";
import { VerifierStatus } from "@/components/VerifierStatus";
import { api } from "@/lib/api";
import type { Network, Verdict } from "@/lib/types";
import { verifyHere, warmUp, type Result } from "@/lib/verifier";
import styles from "../page-shell.module.css";

export default function VerifyPage() {
  const [text, setText] = useState("");
  const [challenge, setChallenge] = useState("");
  const [network, setNetwork] = useState<Network>("main");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Result | null>(null);
  const [serverVerdict, setServerVerdict] = useState<Verdict | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run(receipt: string, expected: string, net: Network = "main") {
    setBusy(true);
    setError(null);
    setResult(null);
    setServerVerdict(null);
    try {
      setResult(await verifyHere(receipt, expected.trim() || undefined, net));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    warmUp();
    // A link of the form /verify#zpd1… carries the receipt in the fragment,
    // which the browser never sends to the server.
    const fromHash = decodeURIComponent(window.location.hash.slice(1));
    if (fromHash.startsWith("zpd")) {
      setText(fromHash);
      void run(fromHash, "");
    }
  }, []);

  async function runOnServer() {
    setError(null);
    try {
      setServerVerdict(await api.verify(text.trim(), challenge.trim() || undefined, network));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function onFile(f: File | undefined) {
    if (!f) return;
    setText((await f.text()).trim());
  }

  return (
    <main className={styles.page}>
      <header className={styles.intro}>
        <p className={styles.eyebrow}>Verify</p>
        <h1 className={styles.title}>Check a payment receipt</h1>
        <p className={styles.lede}>
          Paste the receipt you were sent. It is checked here, in your browser, against the Zcash chain. Add the challenge you issued to make sure it was made for you.
        </p>
        <VerifierStatus />
      </header>

      <form
        className={styles.form}
        onSubmit={(e) => {
          e.preventDefault();
          void run(text, challenge, network);
        }}
      >
        <label className={styles.field}>
          <span>Receipt</span>
          <textarea
            id="receipt"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="zpd1…"
            rows={6}
            spellCheck={false}
            required
            onDrop={(e) => {
              e.preventDefault();
              void onFile(e.dataTransfer.files[0]);
            }}
          />
        </label>
        <div className={styles.row}>
          <label className={styles.field}>
            <span>Your challenge (optional)</span>
            <input id="challenge" value={challenge} onChange={(e) => setChallenge(e.target.value)} placeholder="quittance:v1|…" spellCheck={false} />
          </label>
          <label className={styles.field} style={{ flex: "0 0 11rem" }}>
            <span>Accept payments on</span>
            <select id="network" value={network} onChange={(e) => setNetwork(e.target.value as Network)}>
              <option value="main">Mainnet</option>
              <option value="test">Testnet</option>
            </select>
          </label>
        </div>
        <div className={styles.row}>
          <button type="submit" className={styles.primary} disabled={busy || !text.trim()}>
            {busy ? "Checking…" : "Verify receipt"}
          </button>
          <button type="button" className={styles.ghostButton} onClick={() => void runOnServer()} disabled={!text.trim()}>
            Check on our server instead
          </button>
          <label className={styles.ghost}>
            Open a .zpd file
            <input id="file" type="file" accept=".zpd,.txt" hidden onChange={(e) => void onFile(e.target.files?.[0])} />
          </label>
        </div>
      </form>

      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      {busy && !serverVerdict && (
        <p className={styles.note}>
          Checking in your browser. The first check builds the verifier, which takes a few seconds; later ones are instant. If you can&apos;t wait, check on our server
          instead: it&apos;s faster, but our server then sees the receipt.
        </p>
      )}
      {serverVerdict && !result && (
        <>
          <p className={styles.note}>Checked on our server.{busy ? " The check in your browser is still running and will replace this." : ""}</p>
          <Receipt verdict={serverVerdict} />
        </>
      )}
      {result && <Receipt verdict={result.verdict} lookup={result.lookup} />}
    </main>
  );
}
