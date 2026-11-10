// What the daemon accepts, in one place: `daemon.info` reports these, so clients don't hard-code them.
import type { Limits } from "@ome-labs/api";

import { MAX_ARTIFACT_BYTES } from "./contracts/artifacts.ts";

/** An Aztec entrypoint runs 5 calls (`APP_MAX_CALLS`); the fee payer's always takes one. */
export const LIMITS: Limits = {
  maxCalls: 4,
  maxSummaryBytes: 4096,
  maxArtifactBytes: MAX_ARTIFACT_BYTES,
  memoChars: 280,
  minExpiryHours: 1,
  maxExpiryHours: 720,
};
