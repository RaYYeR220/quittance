"use client";

import { useEffect, useState } from "react";
import { api, type GalleryItem } from "@/lib/api";
import styles from "../page-shell.module.css";

export default function GalleryPage() {
  const [items, setItems] = useState<GalleryItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.gallery().then(setItems, (e: Error) => setError(e.message));
  }, []);

  return (
    <main className={styles.page}>
      <header className={styles.intro}>
        <p className={styles.eyebrow}>Receipts on mainnet</p>
        <h1 className={styles.title}>Real payments, real receipts</h1>
        <p className={styles.lede}>
          Each receipt below proves a real Ironwood payment on Zcash mainnet. Open one and your browser checks it against the chain. Each was signed over the challenge
          shown, so check it with that challenge.
        </p>
      </header>
      {error && <p className={styles.error}>{error}</p>}
      {items?.length === 0 && <p className={styles.note}>No receipts published yet.</p>}
      {items && items.length > 0 && (
        <ol className={styles.panel} style={{ listStyle: "none", margin: 0 }}>
          {items.map((it) => (
            <li key={it.txid + it.challenge} style={{ display: "grid", gap: "0.35rem", paddingBlock: "0.6rem" }}>
              <strong>{it.title}</strong>
              <span className={styles.note}>
                height {it.height.toLocaleString("en-US")} ·{" "}
                <a href={`https://mainnet.zcashexplorer.app/transactions/${it.txid}`} target="_blank" rel="noopener noreferrer">
                  transaction ↗
                </a>{" "}
                · <a href={`/verify#${it.disclosure}`}>verify in this browser</a>
              </span>
              <span className={styles.code}>challenge: {it.challenge}</span>
              {it.note && <span className={styles.note}>{it.note}</span>}
            </li>
          ))}
        </ol>
      )}
    </main>
  );
}
