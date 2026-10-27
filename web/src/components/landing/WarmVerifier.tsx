"use client";

import { useEffect } from "react";
import { warmUp } from "@/lib/verifier";

/**
 * Starts building the verifier while the visitor reads, so /verify and /demo
 * are ready by the time they get there.
 */
export function WarmVerifier() {
  useEffect(() => {
    if ("requestIdleCallback" in window) {
      const id = window.requestIdleCallback(() => warmUp(), { timeout: 5000 });
      return () => window.cancelIdleCallback(id);
    }
    const t = setTimeout(warmUp, 1500);
    return () => clearTimeout(t);
  }, []);
  return null;
}
