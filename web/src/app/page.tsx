import Link from "next/link";
import type { CSSProperties } from "react";
import { MiniField } from "@/components/landing/MiniField";
import { NoteField, type LitNote } from "@/components/landing/NoteField";
import { WarmVerifier } from "@/components/landing/WarmVerifier";
import { DOCS, explorerTx } from "@/lib/links";
import s from "./home.module.css";

/** The first of the demo wallet's mainnet payments, as the receipt in §3 shows it. */
const SAMPLE: LitNote = {
  amount: "0.0002 ZEC",
  to: "u1jfer5uh…zach4a",
  memo: "Invoice 2026-118 · Halcyon Studio",
  height: "3,532,223",
  verifier: "Acme Exchange",
  context: "deposit dispute 8812",
};
const SERIAL = "Nº 3532223";

const MAINNET = [
  { memo: "Invoice 2026-118 · Halcyon Studio", amount: "0.0002 ZEC", height: "3,532,223", txid: "06a8b50f6db826a2533eecfd04fb98beb8df2948494c219efab22d60832f45b6" },
  { memo: "Grant payout · milestone 2 of 4", amount: "0.0003 ZEC", height: "3,532,228", txid: "78df1f3df4259620da37d5c7c9effcf29b519aade405f8d34850fd3c82d06a39" },
  { memo: "Refund · order 5521", amount: "0.00025 ZEC", height: "3,532,234", txid: "580e9621f7fc9625d66c6b8ef844d4aa0d1e0736773ffbf3a1d2baf66d17d51c" },
];

const REFUSALS = [
  { attempt: "Replay a receipt made for someone else", reason: "Bound to your challenge: signed over a different challenge" },
  { attempt: "Change one byte of the output key", reason: "Sender: signed this disclosure: the signature does not cover this disclosure" },
  { attempt: "The recipient claims to be the sender", reason: "the transaction spends none of this wallet’s notes; only the sender of a payment can disclose it" },
  { attempt: "Point it at an Orchard transaction", reason: "Ironwood pool: Orchard proofs are not accepted as evidence (2026 soundness bug)" },
];

const NOT_REVEALED = [
  { line: "The sender’s address or viewing key", width: "82%" },
  { line: "The value of the notes the sender spent", width: "46%" },
  { line: "The change, and every output not disclosed", width: "64%" },
  { line: "Any other transaction of the sender or the recipient", width: "92%" },
];

/** Entrance delay for one element of the hero's load sequence. */
function delay(seconds: number): CSSProperties {
  return { "--d": `${seconds}s` } as CSSProperties;
}

export default function Home() {
  return (
    <main className={s.home}>
      <WarmVerifier />

      {/* 1 · a bearer certificate whose engraving is the field of notes */}
      <section className={s.hero} aria-labelledby="hero-title">
        <div className={s.cert}>
          <NoteField note={SAMPLE} />
          <i className={`${s.corner} ${s.cornerA} ${s.fade}`} />
          <i className={`${s.corner} ${s.cornerB} ${s.fade}`} />
          <i className={`${s.corner} ${s.cornerC} ${s.fade}`} />
          <i className={`${s.corner} ${s.cornerD} ${s.fade}`} />
          <i className={`${s.band} ${s.bandTop} ${s.fade}`} />
          <i className={`${s.band} ${s.bandRight} ${s.fade}`} />
          <i className={`${s.band} ${s.bandBottom} ${s.fade}`} />
          <i className={`${s.band} ${s.bandLeft} ${s.fade}`} />
          <span className={`${s.serial} ${s.serialTop} ${s.fade}`} data-serial aria-hidden="true">
            {SERIAL}
          </span>
          <span className={`${s.serial} ${s.serialBottom} ${s.fade}`} data-serial aria-hidden="true">
            {SERIAL}
          </span>
          <div className={s.claim}>
            <h1 id="hero-title" className={`${s.title} ${s.r}`} style={delay(0.15)}>
              <span data-ink>Prove one shielded payment.</span> <span data-ink>Reveal nothing else.</span>
            </h1>
            <p className={`${s.sub} ${s.r}`} style={delay(0.32)} data-ink>
              Turn a Zcash payment into a receipt anyone can check against the chain: the amount, the address, the memo, and that it was you. Your viewing key never leaves
              your wallet.
            </p>
            <div className={`${s.cta} ${s.r}`} style={delay(0.46)}>
              <Link className={s.button} href="/verify" data-ink>
                Verify a receipt
              </Link>
              <Link className={s.alt} href="/demo" data-ink>
                Try the live demo
              </Link>
            </div>
            <p className={`${s.small} ${s.r}`} style={delay(0.58)} data-ink>
              ZIP 311 for Ironwood · open source · checked in your browser
            </p>
          </div>
        </div>
      </section>

      {/* 2 · the problem */}
      <section className={`${s.sec} ${s.problem}`} aria-labelledby="h-problem">
        <div className={s.pool} aria-hidden="true" style={{ background: "radial-gradient(42vw 30vw at 88% 70%, rgba(20,32,25,.9), rgba(20,32,25,0) 70%)" }} />
        <div className={s.wrap}>
          <div className={s.problemHead}>
            <h2 id="h-problem">Two bad options, until now</h2>
            <p className={s.lede}>
              An exchange’s support desk, a merchant, a grant committee or an auditor asks you to prove a shielded payment. Today you can:
            </p>
          </div>
          <div className={s.options}>
            <div className={s.option}>
              <MiniField mode="all" seed={0} className={s.optionField} />
              <h3>Hand over a viewing key.</h3>
              <p>It proves the payment, and it also shows them every other payment you have ever made or will make.</p>
            </div>
            <div className={s.option}>
              <MiniField mode="none" seed={1} className={s.optionField} />
              <h3>Hand over a txid.</h3>
              <p>It shows nothing, so it proves nothing. Anyone can point at a transaction.</p>
            </div>
            <div className={`${s.option} ${s.answer}`}>
              <div className={`${s.frame} ${s.octagon}`}>
                <div className={`${s.frameInner} ${s.octagon}`}>
                  <MiniField mode="one" seed={2} className={s.optionField} />
                  <h3>A Quittance receipt shows one payment and proves you made it.</h3>
                  <p>It is signed over the verifier’s own challenge, so it can’t be reused anywhere else.</p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* 3 · what a receipt shows */}
      <section className={`${s.sec} ${s.ruled} ${s.record}`} aria-labelledby="h-record">
        <div className={s.pool} aria-hidden="true" style={{ background: "radial-gradient(46vw 34vw at 50% 52%, rgba(21,33,26,.95), rgba(21,33,26,0) 72%)" }} />
        <div className={s.wrap}>
          <h2 id="h-record" className={s.recordTitle}>
            <span>One payment on the record.</span> <span>Everything else stays off it.</span>
          </h2>
          <div className={`${s.card} ${s.frame} ${s.octagon}`}>
            <div className={`${s.cardInner} ${s.frameInner} ${s.octagon}`}>
              <div className={`${s.half} ${s.proven}`}>
                <h3>Proven</h3>
                <ul>
                  <li>
                    <span className={s.check} aria-hidden="true">✓</span>
                    <span className={s.key}>Paid</span>
                    <span className={`${s.value} ${s.amount}`}>{SAMPLE.amount}</span>
                  </li>
                  <li>
                    <span className={s.check} aria-hidden="true">✓</span>
                    <span className={s.key}>To</span>
                    <span className={`${s.value} ${s.mono}`}>{SAMPLE.to}</span>
                  </li>
                  <li>
                    <span className={s.check} aria-hidden="true">✓</span>
                    <span className={s.key}>Memo</span>
                    <span className={`${s.value} ${s.memo}`}>“{SAMPLE.memo}”</span>
                  </li>
                  <li>
                    <span className={s.check} aria-hidden="true">✓</span>
                    <span className={s.key}>Mined at height</span>
                    <span className={`${s.value} ${s.mono}`}>{SAMPLE.height}</span>
                  </li>
                  <li className={s.wide}>
                    <span className={s.check} aria-hidden="true">✓</span>
                    <span className={s.value}>Sent by the holder of the spending key behind this payment</span>
                  </li>
                  <li>
                    <span className={s.check} aria-hidden="true">✓</span>
                    <span className={s.key}>Signed for</span>
                    <span className={`${s.value} ${s.who}`}>
                      {SAMPLE.verifier} · {SAMPLE.context}
                    </span>
                  </li>
                </ul>
              </div>
              <div className={`${s.half} ${s.withheld}`}>
                <MiniField mode="void" seed={3} className={s.voidField} />
                <h3>Not revealed</h3>
                <ul>
                  {NOT_REVEALED.map((n) => (
                    <li key={n.line}>
                      <p>{n.line}</p>
                      <span className={s.void} style={{ width: n.width }} aria-hidden="true" />
                    </li>
                  ))}
                </ul>
              </div>
              <span className={s.cardSerial} aria-hidden="true">
                {SERIAL}
              </span>
            </div>
          </div>
          <p className={s.caption}>Every line above is checked against the chain in the verifier’s browser.</p>
        </div>
      </section>

      {/* 4 · how it works */}
      <section className={`${s.sec} ${s.ruled}`} aria-labelledby="h-steps">
        <div className={s.wrap}>
          <h2 id="h-steps">Three steps, one transaction</h2>
          <ol className={s.steps}>
            <li className={`${s.step} ${s.stepPay}`}>
              <span className={s.station} aria-hidden="true">
                <svg width="28" height="28" viewBox="0 0 28 28">
                  <circle cx="14" cy="14" r="10" fill="none" stroke="#cfd8cc" strokeOpacity=".28" />
                  <circle cx="14" cy="14" r="3.5" fill="#cfd8cc" fillOpacity=".6" />
                </svg>
              </span>
              <span className={s.stepNumber}>1</span>
              <h3>Pay.</h3>
              <p>Send from a wallet that keeps a disclosure kit: the spent note and its path to the payment’s anchor.</p>
            </li>
            <li className={`${s.step} ${s.stepAnswer}`}>
              <span className={s.station} aria-hidden="true">
                <svg width="28" height="28" viewBox="0 0 28 28">
                  <circle cx="14" cy="14" r="10" fill="none" stroke="#c27a4b" strokeOpacity=".9" />
                  <circle cx="14" cy="14" r="3.5" fill="#cfd8cc" fillOpacity=".85" />
                </svg>
              </span>
              <span className={s.stepNumber}>2</span>
              <h3>Answer.</h3>
              <p>When someone asks, re-prove that spend under the same anchor and sign their challenge. Reveal the key that opens the one output you paid.</p>
            </li>
            <li className={`${s.step} ${s.stepCheck}`}>
              <span className={s.station} aria-hidden="true">
                <svg width="28" height="28" viewBox="0 0 28 28">
                  <defs>
                    <radialGradient id="station-glow">
                      <stop offset="0" stopColor="#c27a4b" stopOpacity=".45" />
                      <stop offset="1" stopColor="#c27a4b" stopOpacity="0" />
                    </radialGradient>
                  </defs>
                  <circle cx="14" cy="14" r="14" fill="url(#station-glow)" />
                  <circle cx="14" cy="14" r="10" fill="none" stroke="#c27a4b" />
                  <circle cx="14" cy="14" r="4" fill="#d99466" />
                </svg>
              </span>
              <span className={s.stepNumber}>3</span>
              <h3>Check.</h3>
              <p>The verifier’s browser fetches the transaction, checks the proof, the signature and the output, and shows what is proven and what isn’t.</p>
            </li>
          </ol>
        </div>
      </section>

      {/* 5 · try to break it */}
      <section className={s.sec} aria-labelledby="h-break">
        <div className={s.pool} aria-hidden="true" style={{ background: "radial-gradient(40vw 30vw at 12% 40%, rgba(20,32,25,.85), rgba(20,32,25,0) 70%)" }} />
        <div className={s.wrap}>
          <h2 id="h-break">Every forgery is refused, with the reason</h2>
          <ul className={s.slips}>
            {REFUSALS.map((r) => (
              <li key={r.attempt} className={`${s.slip} ${s.octagon}`}>
                <span className={s.holes} aria-hidden="true" />
                <p className={s.attempt}>{r.attempt}</p>
                <p className={s.refusal}>
                  <span className={s.cross} aria-hidden="true">
                    ✕
                  </span>
                  <span className="visually-hidden">Refused: </span>
                  <code>{r.reason}</code>
                </p>
              </li>
            ))}
          </ul>
          <p className={s.after}>
            <Link className={s.more} href="/demo">
              Try each one in the live demo →
            </Link>
          </p>
        </div>
      </section>

      {/* 6 · under the hood */}
      <section className={`${s.sec} ${s.ruled}`} aria-labelledby="h-hood">
        <div className={`${s.wrap} ${s.hood}`}>
          <div>
            <h2 id="h-hood">Nothing new to trust</h2>
            <p className={`${s.lede} ${s.hoodLede}`}>
              A receipt is itself an Ironwood bundle that can never be mined. It re-proves the spend with the same Halo2 circuit, signs with the same RedPallas keys and is
              checked with the same verifying key consensus uses. No new circuit, no trusted setup.
            </p>
            <p className={s.note}>Specified as an Ironwood addendum to ZIP 311, which covers Sapling and leaves Orchard as future work.</p>
            <a className={s.more} href={DOCS.addendum}>
              Read the addendum →
            </a>
          </div>
          <dl className={s.serials}>
            <div className={s.figure}>
              <dt>the spend proof in a receipt</dt>
              <dd>
                <span className={s.figureNumber}>4,992</span>
                <span className={s.figureUnit}>bytes</span>
              </dd>
            </div>
            <div className={s.figure}>
              <dt>to make one on a laptop</dt>
              <dd>
                <span className={s.figureNumber}>0.6</span>
                <span className={s.figureUnit}>s</span>
              </dd>
            </div>
            <div className={s.figure}>
              <dt>to check it, native</dt>
              <dd>
                <span className={s.figureNumber}>6</span>
                <span className={s.figureUnit}>ms</span>
              </dd>
            </div>
          </dl>
        </div>
      </section>

      {/* 7 · use it */}
      <section className={s.sec} aria-labelledby="h-use">
        <div className={s.pool} aria-hidden="true" style={{ background: "radial-gradient(44vw 34vw at 80% 30%, rgba(20,32,25,.85), rgba(20,32,25,0) 70%)" }} />
        <div className={`${s.wrap} ${s.use}`}>
          <div className={s.useHead}>
            <h2 id="h-use">Wherever proof gets asked for</h2>
          </div>
          <div className={s.plates}>
            <Link href="/docs#cli" className={`${s.plate} ${s.octagon}`}>
              <div>
                <h3>Command line</h3>
              </div>
              <pre>
                <code>
                  <span className={s.prompt}>$ </span>quittance prove <span className={s.nowrap}>--txid …</span> <span className={s.nowrap}>--challenge …</span>
                  {"\n"}
                  <span className={s.prompt}>$ </span>quittance verify zpd1…
                </code>
              </pre>
            </Link>
            <Link href="/docs#browser" className={`${s.plate} ${s.octagon}`}>
              <div>
                <h3>In the browser</h3>
                <p>The verifier compiled to WebAssembly</p>
              </div>
              <pre>
                <code>verify(receipt, tx, height, challenge)</code>
              </pre>
            </Link>
            <Link href="/docs#api" className={`${s.plate} ${s.octagon}`}>
              <div>
                <h3>HTTP API</h3>
                <p>For support desks</p>
              </div>
              <pre>
                <code>{"POST /v1/verify\nPOST /v1/challenges"}</code>
              </pre>
            </Link>
            <Link href="/docs#agents" className={`${s.plate} ${s.octagon}`}>
              <div>
                <h3>For agents</h3>
                <p>An MCP server</p>
              </div>
              <pre>
                <code>{"create_payment_challenge\nverify_payment_disclosure"}</code>
              </pre>
            </Link>
          </div>
        </div>
      </section>

      {/* 8 · on mainnet */}
      <section className={`${s.sec} ${s.ruled}`} aria-labelledby="h-mainnet">
        <div className={s.wrap}>
          <div className={s.mainnetHead}>
            <h2 id="h-mainnet">Real payments, real receipts</h2>
            <p>58 automated tests, including every refusal above, three of them against these receipts.</p>
          </div>
          <ul className={s.ledger}>
            {MAINNET.map((r) => (
              <li key={r.txid}>
                <span className={s.dot} aria-hidden="true" />
                <span className={s.ledgerMemo}>{r.memo}</span>
                <span className={s.ledgerAmount}>{r.amount}</span>
                <span className={s.ledgerHeight}>height {r.height}</span>
                <a href={explorerTx("main", r.txid)} target="_blank" rel="noopener noreferrer">
                  view transaction ↗<span className="visually-hidden"> for {r.memo} (opens in a new tab)</span>
                </a>
              </li>
            ))}
          </ul>
        </div>
      </section>
    </main>
  );
}
