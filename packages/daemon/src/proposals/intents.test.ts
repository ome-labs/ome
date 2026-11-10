// A proposal's summary carries its intent: a kind a newer release wrote reads as unknown (shown by its decoded calls),
// and text that isn't an intent reads as a memo.
import { describe, expect, it } from "vitest";

import { intentOf } from "./intents.ts";

describe("intentOf", () => {
  it("reads known kinds, a newer release's kind as unknown, and anything else as a memo", () => {
    expect(intentOf(JSON.stringify({ kind: "invalidatePending", memo: null }))).toEqual({
      kind: "invalidatePending",
      memo: null,
    });
    expect(intentOf(JSON.stringify({ kind: "batch", payments: [], memo: "payroll" }))).toEqual({
      kind: "unknown",
    });
    expect(intentOf("pay the auditors")).toEqual({
      kind: "calls",
      calls: [],
      memo: "pay the auditors",
    });
  });
});
