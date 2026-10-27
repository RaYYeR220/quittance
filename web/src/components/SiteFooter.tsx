import Link from "next/link";
import { DOCS, REPO } from "@/lib/links";
import styles from "./SiteFooter.module.css";

export function SiteFooter() {
  return (
    <footer className={styles.foot}>
      <div className={styles.strip} aria-hidden="true" />
      <div className={styles.wrap}>
        <Link href="/" className={styles.brand}>
          Quittance
        </Link>
        <nav aria-label="Footer" className={styles.links}>
          <a href={DOCS.licence}>MIT licence</a>
          <a href={REPO}>GitHub</a>
          <a href={DOCS.addendum}>ZIP 311 addendum</a>
          <a href={DOCS.threatModel}>Threat model</a>
          <a href={DOCS.proof}>Proof</a>
        </nav>
        <p className={styles.fine}>Ironwood only. Orchard-pool payments are refused on purpose. Sapling isn’t implemented.</p>
      </div>
    </footer>
  );
}
