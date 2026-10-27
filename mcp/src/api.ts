/** A thin client for the Quittance HTTP API. */

export type Network = "main" | "test";

export interface Check {
  id: string;
  label: string;
  status: "pass" | "fail" | "warn" | "skip";
  detail: string;
}

export interface Payment {
  action_index: number;
  value_zat: number;
  value_zec: string;
  recipient: string;
  memo: { kind: "empty" } | { kind: "text"; value: string } | { kind: "bytes"; value: string };
}

export interface Verdict {
  valid: boolean;
  /** Signed over the challenge the caller passed; false when none was passed. */
  bound: boolean;
  /** The challenge hasn't expired. */
  fresh: boolean;
  network: Network;
  txid: string;
  height: number;
  challenge_text: string | null;
  checks: Check[];
  revealed: { payments: Payment[]; spends: { action_index: number; nullifier: string; mode: string }[] } | null;
  not_revealed: string[];
}

export interface ChallengeTicket {
  id: string;
  network: Network;
  msg: string;
  expires: number;
  answer_url: string;
  status: "pending" | "answered";
  verdict?: Verdict;
}

export class QuittanceApi {
  constructor(
    private readonly base: string,
    private readonly fetchImpl: typeof fetch = fetch,
  ) {}

  private async call<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await this.fetchImpl(new URL(path, this.base), {
      ...init,
      headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
    });
    const body = await res.text();
    if (!res.ok) {
      let reason = body;
      try {
        reason = (JSON.parse(body) as { error?: string }).error ?? body;
      } catch {
        // Not JSON: keep the raw text.
      }
      throw new Error(`Quittance API ${res.status}: ${reason}`);
    }
    return JSON.parse(body) as T;
  }

  verify(disclosure: string, challenge?: string, network: Network = "main"): Promise<Verdict> {
    return this.call("/v1/verify", { method: "POST", body: JSON.stringify({ disclosure, challenge, network }) });
  }

  createChallenge(input: { verifier: string; context: string; ttl_seconds?: number; network?: Network }): Promise<ChallengeTicket> {
    return this.call("/v1/challenges", { method: "POST", body: JSON.stringify(input) });
  }

  getChallenge(id: string): Promise<ChallengeTicket> {
    return this.call(`/v1/challenges/${encodeURIComponent(id)}`);
  }
}

/** One sentence a support agent can act on, followed by the evidence. */
export function describeVerdict(v: Verdict): string {
  const lines: string[] = [];
  const net = v.network === "main" ? "Zcash mainnet" : "Zcash testnet (test coins, no value)";
  const unit = v.network === "main" ? "ZEC" : "TAZ";
  if (v.valid && v.revealed) {
    for (const p of v.revealed.payments) {
      const memo = p.memo.kind === "text" ? ` with memo "${p.memo.value}"` : p.memo.kind === "bytes" ? " with a binary memo" : "";
      lines.push(
        `VALID on ${net}: the holder of a spending key used in this payment paid ${p.value_zec} ${unit} to ${p.recipient}${memo}, in transaction ${v.txid} (height ${v.height}).`,
      );
    }
    if (!v.bound) {
      lines.push("NOT BOUND to a challenge of yours: no challenge was checked, so this could be a receipt made for someone else. Ask for a fresh one.");
    } else if (!v.fresh) {
      lines.push("The challenge it answers has expired.");
    }
    if (v.revealed.payments.length === 0) {
      lines.push(`VALID, but no output is disclosed: it proves who spent in ${v.txid}, not whom they paid.`);
    }
  } else {
    const first = v.checks.find((c) => c.status === "fail");
    lines.push(`REFUSED: ${first ? `${first.label}: ${first.detail}` : "the disclosure did not verify"}.`);
  }
  for (const c of v.checks) {
    const mark = c.status === "pass" ? "ok  " : c.status === "fail" ? "FAIL" : c.status === "warn" ? "warn" : "skip";
    lines.push(`  [${mark}] ${c.label}: ${c.detail}`);
  }
  if (v.valid) {
    lines.push(`Not revealed: ${v.not_revealed.join("; ")}.`);
  }
  return lines.join("\n");
}
