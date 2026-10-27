"use client";

import { useEffect, useState } from "react";
import { onReady, warmUp } from "@/lib/verifier";
import styles from "./VerifierStatus.module.css";

/**
 * The verifying key is derived from the circuit in this browser rather than
 * downloaded, which takes a few seconds the first time; this says so.
 */
export function VerifierStatus() {
  const [ms, setMs] = useState<number | null>(null);
  useEffect(() => {
    warmUp();
    return onReady(setMs);
  }, []);
  return (
    <p className={styles.status} data-ready={ms !== null}>
      <span className={styles.lamp} aria-hidden="true" />
      {ms === null
        ? "Building the verifier in your browser from the Ironwood circuit…"
        : `Verifier ready. Its key was derived here from the circuit in ${(ms / 1000).toFixed(1)} s; nothing was downloaded to trust.`}
    </p>
  );
}
