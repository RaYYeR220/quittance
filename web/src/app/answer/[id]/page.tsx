"use client";

import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import { CopyButton } from "@/components/CopyButton";
import { Receipt } from "@/components/Receipt";
import { api } from "@/lib/api";
import type { Ticket } from "@/lib/types";
import styles from "../../page-shell.module.css";

export default function AnswerPage() {
  const { id } = useParams<{ id: string }>();
  const [ticket, setTicket] = useState<Ticket | null>(null);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.challenge(id).then(setTicket, (e: Error) => setError(e.message));
  }, [id]);

  async function send(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      setTicket(await api.answer(id, text.trim()));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  if (!ticket) return <main className={styles.page}>{error ? <p className={styles.error}>{error}</p> : <p className={styles.note}>Loading…</p>}</main>;

  const cmd = `quittance prove --dir <your wallet> --txid <the payment's txid> --challenge '${ticket.msg}'`;
  return (
    <main className={styles.page}>
      <header className={styles.intro}>
        <p className={styles.eyebrow}>Answer a request</p>
        <h1 className={styles.title}>
          {ticket.verifier} asks you to prove a payment
        </h1>
        <p className={styles.lede}>
          For: {ticket.context}. Your receipt will show that one payment and that you made it. It won&apos;t show your address, your balance, your change or anything else
          you&apos;ve done.
        </p>
      </header>
      <section className={styles.panel}>
        <p className={styles.label}>1. Make the receipt in your wallet</p>
        <p className={styles.code}>{cmd}</p>
        <CopyButton text={cmd} label="Copy command" />
        <p className={styles.note}>The keys never leave your machine. This page never asks for them.</p>
      </section>
      <form className={styles.form} onSubmit={send}>
        <label className={styles.field}>
          <span>2. Paste the receipt</span>
          <textarea id="answer" value={text} onChange={(e) => setText(e.target.value)} rows={5} placeholder="zpd1…" spellCheck={false} required />
        </label>
        <button type="submit" className={styles.primary} disabled={busy || !text.trim()}>
          {busy ? "Sending…" : "Send it to " + ticket.verifier}
        </button>
        {error && (
          <p className={styles.error} role="alert">
            {error}
          </p>
        )}
      </form>
      {ticket.verdict && <Receipt verdict={ticket.verdict} />}
    </main>
  );
}
