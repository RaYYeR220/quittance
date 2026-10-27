"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { api } from "@/lib/api";
import type { Network } from "@/lib/types";
import styles from "../page-shell.module.css";

export default function AskPage() {
  const router = useRouter();
  const [verifier, setVerifier] = useState("");
  const [context, setContext] = useState("");
  const [hours, setHours] = useState(24);
  const [network, setNetwork] = useState<Network>("main");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function create(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const t = await api.createChallenge({ verifier, context, ttl_seconds: hours * 3600, network });
      router.push(`/ask/${t.id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  }

  return (
    <main className={styles.page}>
      <header className={styles.intro}>
        <p className={styles.eyebrow}>Ask for proof</p>
        <h1 className={styles.title}>Ask someone to prove a payment</h1>
        <p className={styles.lede}>
          You get a link to send them. Their receipt will be signed over a challenge only you issued, so they can&apos;t reuse one made for anyone else. You&apos;ll see the
          result on the next page as soon as they answer.
        </p>
      </header>
      <form className={styles.form} onSubmit={create}>
        <div className={styles.row}>
          <label className={styles.field}>
            <span>Who is asking</span>
            <input id="verifier" value={verifier} onChange={(e) => setVerifier(e.target.value)} placeholder="Acme Exchange" maxLength={200} required />
          </label>
          <label className={styles.field}>
            <span>What the proof is for</span>
            <input id="context" value={context} onChange={(e) => setContext(e.target.value)} placeholder="deposit dispute 8812" maxLength={200} required />
          </label>
        </div>
        <div className={styles.row}>
          <label className={styles.field}>
            <span>Valid for</span>
            <select id="hours" value={hours} onChange={(e) => setHours(Number(e.target.value))}>
              <option value={1}>1 hour</option>
              <option value={24}>24 hours</option>
              <option value={168}>7 days</option>
            </select>
          </label>
          <label className={styles.field}>
            <span>Network</span>
            <select id="network" value={network} onChange={(e) => setNetwork(e.target.value as Network)}>
              <option value="main">Mainnet</option>
              <option value="test">Testnet</option>
            </select>
          </label>
        </div>
        <p className={styles.note}>Don&apos;t put personal data in these fields: they are stored with the challenge and shown to the payer.</p>
        <button type="submit" className={styles.primary} disabled={busy}>
          {busy ? "Creating…" : "Create the link"}
        </button>
        {error && (
          <p className={styles.error} role="alert">
            {error}
          </p>
        )}
      </form>
    </main>
  );
}
