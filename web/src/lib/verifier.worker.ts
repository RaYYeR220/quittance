/// <reference lib="webworker" />
/**
 * Runs the WebAssembly verifier off the main thread. Building the verifying
 * key from the circuit takes several seconds, so it starts as soon as the
 * worker loads and every request waits for it.
 */

type WasmModule = {
  default: (input?: { module_or_path: string | URL }) => Promise<unknown>;
  prepare: () => void;
  inspect: (text: string) => string;
  verify: (text: string, rawTxHex: string, minedHeight: number | undefined, expected: string | undefined, network: string | undefined, now: number) => string;
  tamper: (text: string, how: string, newChallenge: string | undefined) => string;
};

export type Request =
  | { id: number; op: "inspect"; text: string }
  | { id: number; op: "verify"; text: string; rawTxHex: string; minedHeight?: number; expected?: string; network?: string }
  | { id: number; op: "tamper"; text: string; how: string; newChallenge?: string };

export type Response =
  | { kind: "result"; id: number; ok: true; value: string }
  | { kind: "result"; id: number; ok: false; error: string }
  | { kind: "ready"; ms: number };

const ctx = self as unknown as DedicatedWorkerGlobalScope;

const loading: Promise<WasmModule> = (async () => {
  const url = new URL("/wasm/quittance.js", ctx.location.origin).href;
  const mod = (await import(/* webpackIgnore: true */ /* @vite-ignore */ url)) as WasmModule;
  await mod.default({ module_or_path: new URL("/wasm/quittance_bg.wasm", ctx.location.origin) });
  const t = performance.now();
  mod.prepare();
  ctx.postMessage({ kind: "ready", ms: Math.round(performance.now() - t) } satisfies Response);
  return mod;
})();

ctx.onmessage = async (e: MessageEvent<Request>) => {
  const req = e.data;
  try {
    const mod = await loading;
    let value: string;
    switch (req.op) {
      case "inspect":
        value = mod.inspect(req.text);
        break;
      case "verify":
        value = mod.verify(req.text, req.rawTxHex, req.minedHeight, req.expected, req.network, Date.now() / 1000);
        break;
      case "tamper":
        value = mod.tamper(req.text, req.how, req.newChallenge);
        break;
    }
    ctx.postMessage({ kind: "result", id: req.id, ok: true, value } satisfies Response);
  } catch (err) {
    ctx.postMessage({ kind: "result", id: req.id, ok: false, error: err instanceof Error ? err.message : String(err) } satisfies Response);
  }
};
