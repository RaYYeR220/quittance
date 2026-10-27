import type { Metadata, Viewport } from "next";
import { Azeret_Mono, Playfair, Public_Sans } from "next/font/google";
import type { CSSProperties } from "react";
import { SiteFooter } from "@/components/SiteFooter";
import { SiteNav } from "@/components/SiteNav";
import { rope } from "@/lib/rope";
import "./globals.css";

const display = Playfair({ subsets: ["latin"], axes: ["opsz"], variable: "--font-display", display: "swap" });
const sans = Public_Sans({ subsets: ["latin"], variable: "--font-sans", display: "swap" });
const mono = Azeret_Mono({ subsets: ["latin"], variable: "--font-mono", display: "swap" });

export const metadata: Metadata = {
  title: { default: "Quittance", template: "%s · Quittance" },
  description: "Prove one shielded Zcash payment without handing over a viewing key. ZIP 311 payment disclosures for the Ironwood pool, checked in your browser.",
};

export const viewport: Viewport = {
  themeColor: "#0d1411",
  colorScheme: "dark",
};

/** The certificate's rope border, shared by the hero frame and the footer strip. */
const ropeVars = { "--rope-h": rope.horizontal, "--rope-v": rope.vertical } as CSSProperties;

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className={`${display.variable} ${sans.variable} ${mono.variable}`} style={ropeVars}>
      <body>
        <a className="skip-link" href="#content">
          Skip to content
        </a>
        <SiteNav />
        <div id="content">{children}</div>
        <SiteFooter />
      </body>
    </html>
  );
}
