"use client";

import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import { CopyButton } from "@/components/CopyButton";
import { Receipt } from "@/components/Receipt";
import { api } from "@/lib/api";
import type { Ticket } from "@/lib/types";
import styles from "../../page-shell.module.css";

export default function AskStatusPage() {
  const { id } = useParams<{ id: string }>();
  const [ticket, setTicket] = useState<Ticket | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let stop = false;
    async function poll() {
      try {
        const t = await api.challenge(id);
        if (stop) return;
        setTicket(t);
        const settled = t.verdict?.valid === true;
        if (!settled) setTimeout(poll, 4000);
      } catch (e) {
        if (!stop) setError(e instanceof Error ? e.message : String(e));
      }
    }
    void poll();
    return () => {
      stop = true;
    };
  }, [id]);

  if (error) return <main className={styles.page}><p className={styles.error}>{error}</p></main>;
  if (!ticket) return <main className={styles.page}><p className={styles.note}>Loading…</p></main>;

  const answerUrl = `${window.location.origin}/answer/${ticket.id}`;
  return (
    <main className={styles.page}>
      <header className={styles.intro}>
        <p className={styles.eyebrow}>Ask for proof · {ticket.verifier}</p>
        <h1 className={styles.title}>{ticket.status === "answered" ? "They answered" : "Waiting for the payer"}</h1>
        <p className={styles.lede}>
          Send this link to the person who paid. It shows them your challenge and how to answer it. This page updates on its own.
        </p>
      </header>
      <section className={styles.panel}>
        <p className={styles.label}>Link for the payer</p>
        <p className={styles.code}>{answerUrl}</p>
        <CopyButton text={answerUrl} label="Copy link" />
        <p className={styles.label}>Your challenge</p>
        <p className={styles.code}>{ticket.msg}</p>
        <p className={styles.note}>Expires {new Date(ticket.expires * 1000).toLocaleString()}.</p>
      </section>
      {ticket.verdict && <Receipt verdict={ticket.verdict} />}
      {ticket.disclosure && (
        <section className={styles.panel}>
          <p className={styles.label}>The receipt they sent (keep it as evidence)</p>
          <CopyButton text={ticket.disclosure} label="Copy receipt" />
        </section>
      )}
    </main>
  );
}
