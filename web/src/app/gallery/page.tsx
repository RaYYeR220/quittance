"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { api, type GalleryItem } from "@/lib/api";
import { explorerTx } from "@/lib/links";
import styles from "../page-shell.module.css";
import gallery from "./gallery.module.css";

/** "Invoice 2026-118 · Halcyon Studio · 0.0002 ZEC" → the memo and the amount, shown apart. */
function splitTitle(title: string): { memo: string; amount: string | null } {
  const parts = title.split(" · ");
  const last = parts[parts.length - 1];
  if (parts.length > 1 && /\b(ZEC|TAZ)$/.test(last)) return { memo: parts.slice(0, -1).join(" · "), amount: last };
  return { memo: title, amount: null };
}

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
      {!items && !error && <p className={styles.note}>Loading the receipts…</p>}
      {items?.length === 0 && <p className={styles.note}>No receipts published yet.</p>}
      {items && items.length > 0 && (
        <div className={gallery.strip}>
          <ol className={gallery.coupons}>
            {items.map((it) => {
              const { memo, amount } = splitTitle(it.title);
              return (
                <li key={it.txid + it.challenge} className={gallery.coupon}>
                  <span className={gallery.dot} aria-hidden="true" />
                  <h2 className={gallery.memo}>{memo}</h2>
                  <p className={gallery.meta}>
                    {amount && <span className={gallery.amount}>{amount}</span>}
                    <span>height {it.height.toLocaleString("en-US")}</span>
                  </p>
                  <p className={gallery.challenge}>
                    <span className={gallery.challengeLabel}>Signed over</span>
                    <code>{it.challenge}</code>
                  </p>
                  {it.note && <p className={gallery.note}>{it.note}</p>}
                  <p className={gallery.links}>
                    <Link href={`/verify#${it.disclosure}`} className={gallery.verify}>
                      Verify in this browser
                    </Link>
                    <a href={explorerTx(it.network, it.txid)} target="_blank" rel="noopener noreferrer">
                      transaction ↗
                    </a>
                  </p>
                </li>
              );
            })}
          </ol>
        </div>
      )}
    </main>
  );
}
