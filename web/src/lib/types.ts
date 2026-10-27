export type Network = "main" | "test";

export type CheckStatus = "pass" | "fail" | "warn" | "skip";

export interface Check {
  id: string;
  label: string;
  status: CheckStatus;
  detail: string;
}

export type Memo = { kind: "empty" } | { kind: "text"; value: string } | { kind: "bytes"; value: string };

export interface Payment {
  action_index: number;
  value_zat: number;
  value_zec: string;
  recipient: string;
  memo: Memo;
}

export interface Challenge {
  verifier: string;
  context: string;
  nonce: number[];
  expires: number;
}

export interface Verdict {
  valid: boolean;
  /** Signed over the challenge the verifier gave; false when none was given. */
  bound: boolean;
  /** The challenge hasn't expired. */
  fresh: boolean;
  network: Network;
  txid: string;
  height: number;
  challenge_text: string | null;
  challenge: Challenge | null;
  checks: Check[];
  revealed: { payments: Payment[]; spends: { action_index: number; nullifier: string; mode: string }[] } | null;
  not_revealed: string[];
}

export interface Inspection {
  network: Network;
  txid: string;
  height: number;
  challenge_text: string | null;
  challenge: Challenge | null;
  spends: number;
  outputs: number;
  proof_bytes: number;
  has_tx: boolean;
  bytes: number;
}

export interface Ticket {
  id: string;
  network: Network;
  msg: string;
  verifier: string;
  context: string;
  created: number;
  expires: number;
  answer_url: string;
  status: "pending" | "answered";
  answered: number | null;
  disclosure: string | null;
  verdict: Verdict | null;
}
