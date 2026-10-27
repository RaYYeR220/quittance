import type { Metadata } from "next";
import Link from "next/link";
import "./globals.css";

export const metadata: Metadata = {
  title: { default: "Quittance", template: "%s · Quittance" },
  description: "Prove one shielded Zcash payment without handing over a viewing key. ZIP 311 payment disclosures for the Ironwood pool.",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en">
      <body>
        <nav className="site-nav" aria-label="Main">
          <Link href="/" className="brand">
            Quittance
          </Link>
          <div className="links">
            <Link href="/verify">Verify</Link>
            <Link href="/ask">Ask for proof</Link>
            <Link href="/demo">Demo</Link>
            <Link href="/docs">Docs</Link>
            <a href="https://github.com/RaYYeR220/quittance">GitHub</a>
          </div>
        </nav>
        {children}
      </body>
    </html>
  );
}
