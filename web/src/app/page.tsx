import Link from "next/link";

export default function Home() {
  return (
    <main style={{ padding: "4rem 1rem", maxWidth: "48rem", margin: "0 auto" }}>
      <h1>Prove one shielded payment. Reveal nothing else.</h1>
      <p>
        <Link href="/verify">Verify a receipt</Link> · <Link href="/demo">Try the live demo</Link>
      </p>
    </main>
  );
}
