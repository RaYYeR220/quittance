"use client";

import { api } from "./api";
import type { Inspection, Verdict } from "./types";
import type { Request, Response } from "./verifier.worker";

type Pending = { resolve: (v: string) => void; reject: (e: Error) => void };

let worker: Worker | null = null;
let nextId = 1;
const pending = new Map<number, Pending>();
let readyMs: number | null = null;
const readyListeners = new Set<(ms: number) => void>();

function boot(): Worker {
  if (worker) return worker;
  worker = new Worker(new URL("./verifier.worker.ts", import.meta.url), { type: "module" });
  worker.onmessage = (e: MessageEvent<Response>) => {
    const msg = e.data;
    if (msg.kind === "ready") {
      readyMs = msg.ms;
      readyListeners.forEach((f) => f(msg.ms));
      return;
    }
    const p = pending.get(msg.id);
    if (!p) return;
    pending.delete(msg.id);
    if (msg.ok) p.resolve(msg.value);
    else p.reject(new Error(msg.error));
  };
  return worker;
}

/** Starts building the verifying key in the background. Safe to call often. */
export function warmUp(): void {
  if (typeof window !== "undefined") boot();
}

/** Calls `f` once the verifier is ready (immediately if it already is). */
export function onReady(f: (ms: number) => void): () => void {
  if (readyMs !== null) {
    f(readyMs);
    return () => {};
  }
  readyListeners.add(f);
  return () => readyListeners.delete(f);
}

type WithoutId<T> = T extends unknown ? Omit<T, "id"> : never;

function rpc(req: WithoutId<Request>): Promise<string> {
  const w = boot();
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    w.postMessage({ ...req, id });
  });
}

export async function inspect(text: string): Promise<Inspection> {
  return JSON.parse(await rpc({ op: "inspect", text })) as Inspection;
}

export async function tamper(text: string, how: "output-key" | "proof" | "signature" | "challenge", newChallenge?: string): Promise<string> {
  return rpc({ op: "tamper", text, how, newChallenge });
}

/** How the page confirmed the transaction is mined, i.e. what our API learned. */
export type ChainLookup = { kind: "block"; height: number } | { kind: "txid"; txid: string };

export interface Result {
  inspection: Inspection;
  verdict: Verdict;
  lookup: ChainLookup;
}

/**
 * Verifies a disclosure in this browser. If the prover attached the
 * transaction, the API is asked only for the txids of the claimed block;
 * otherwise it is asked for the transaction by txid.
 */
export async function verifyHere(text: string, expectedChallenge?: string): Promise<Result> {
  const inspection = await inspect(text);
  const { network, txid, height } = inspection;
  let raw = "";
  let mined: number | undefined;
  let lookup: ChainLookup;
  if (inspection.has_tx) {
    lookup = { kind: "block", height };
    const block = await api.block(network, height);
    mined = block.txids.includes(txid) ? height : undefined;
  } else {
    lookup = { kind: "txid", txid };
    const tx = await api.tx(network, txid);
    raw = tx.raw;
    mined = tx.height ?? undefined;
  }
  const verdict = JSON.parse(
    await rpc({ op: "verify", text, rawTxHex: raw, minedHeight: mined, expected: expectedChallenge || undefined, network }),
  ) as Verdict;
  return { inspection, verdict, lookup };
}
