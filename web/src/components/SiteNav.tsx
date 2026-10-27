"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useRef, useState } from "react";
import { REPO } from "@/lib/links";
import styles from "./SiteNav.module.css";

const ROUTES = [
  { href: "/verify", label: "Verify" },
  { href: "/ask", label: "Ask for proof" },
  { href: "/demo", label: "Demo" },
  { href: "/docs", label: "Docs" },
] as const;

export function SiteNav() {
  const pathname = usePathname();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLElement>(null);

  // On small screens the links fold into a menu; it closes on Escape or a click elsewhere.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    const onPointer = (e: PointerEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onPointer);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onPointer);
    };
  }, [open]);

  return (
    <header className={styles.top} ref={root} data-open={open}>
      <Link href="/" className={styles.brand}>
        Quittance
      </Link>
      <button type="button" className={styles.menu} aria-expanded={open} aria-controls="site-links" onClick={() => setOpen((o) => !o)}>
        {open ? "Close" : "Menu"}
      </button>
      <nav id="site-links" className={styles.links} aria-label="Main">
        {ROUTES.map((r) => (
          <Link key={r.href} href={r.href} aria-current={pathname.startsWith(r.href) ? "page" : undefined} onClick={() => setOpen(false)}>
            {r.label}
          </Link>
        ))}
        <a href={REPO} onClick={() => setOpen(false)}>
          GitHub
        </a>
      </nav>
    </header>
  );
}
