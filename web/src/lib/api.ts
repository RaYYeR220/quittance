import type { Network, Ticket, Verdict } from "./types";

export const API = (process.env.NEXT_PUBLIC_QUITTANCE_API ?? "http://127.0.0.1:8080").replace(/\/$/, "");

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API}${path}`, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  const text = await res.text();
  let body: unknown = null;
  try {
    body = JSON.parse(text);
  } catch {
    // Not JSON.
  }
  if (!res.ok) {
    const reason = (body as { error?: string } | null)?.error ?? (text || res.statusText);
    throw new ApiError(res.status, reason);
  }
  return body as T;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export const api = {
  tx: (network: Network, txid: string) => call<{ txid: string; height: number | null; raw: string }>(`/v1/tx/${network}/${txid}`),
  block: (network: Network, height: number) => call<{ height: number; txids: string[] }>(`/v1/block/${network}/${height}`),
  verify: (disclosure: string, challenge?: string) =>
    call<Verdict>("/v1/verify", { method: "POST", body: JSON.stringify({ disclosure, challenge }) }),
  createChallenge: (input: { verifier: string; context: string; ttl_seconds?: number; network?: Network }) =>
    call<Ticket>("/v1/challenges", { method: "POST", body: JSON.stringify(input) }),
  challenge: (id: string) => call<Ticket>(`/v1/challenges/${encodeURIComponent(id)}`),
  answer: (id: string, disclosure: string) =>
    call<Ticket>(`/v1/challenges/${encodeURIComponent(id)}/answer`, { method: "POST", body: JSON.stringify({ disclosure }) }),
  demo: () => call<DemoInfo>("/v1/demo"),
  demoProve: (challenge: string, payment = 0) =>
    call<DemoProof>("/v1/demo/prove", { method: "POST", body: JSON.stringify({ challenge, payment }) }),
  demoForge: (challenge: string, payment = 0) =>
    call<DemoForgery>("/v1/demo/forge", { method: "POST", body: JSON.stringify({ challenge, payment }) }),
  demoOrchard: (disclosure: string) => call<{ disclosure: string }>("/v1/demo/orchard", { method: "POST", body: JSON.stringify({ disclosure }) }),
  gallery: () => call<GalleryItem[]>("/v1/gallery"),
};

export interface DemoPayment {
  txid: string;
  height: number;
  value_zec: string;
  memo: string;
  recipient: string;
}

export interface DemoInfo {
  network: Network;
  payments: DemoPayment[];
  orchard_txid?: string;
}

export interface DemoProof {
  disclosure: string;
  payment: DemoPayment;
  prove_ms: number;
}

export interface DemoForgery {
  refused: boolean;
  reason: string;
}

export interface GalleryItem {
  title: string;
  network: Network;
  txid: string;
  height: number;
  challenge: string;
  disclosure: string;
  note?: string;
}
