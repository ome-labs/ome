// A balance read is a value, "too few keepers" with their numbers (however Aztec wrapped the keeper's error), or the
// error verbatim.
import { describe, expect, it } from "vitest";

import { DaemonError } from "../errors.ts";
import { balanceState } from "./tokens.ts";

describe("balance states", () => {
  it("is the amount when the read succeeds", async () => {
    expect(await balanceState(() => Promise.resolve(42n))).toEqual({
      state: "known",
      amount: "42",
    });
  });

  it("names how many keepers answered, from the keeper's details or its message", async () => {
    const detailed = new Error("simulation failed", {
      cause: DaemonError.detailed(
        { kind: "unreadable", answered: 1, needed: 2 },
        "only 1 of 2 members answered",
      ),
    });
    expect(await balanceState(() => Promise.reject(detailed))).toEqual({
      state: "unreadable",
      answered: 1,
      needed: 2,
    });
    const wrapped = new Error("Failed to execute utility: keeper: only 2 of 3 members answered");
    expect(await balanceState(() => Promise.reject(wrapped))).toEqual({
      state: "unreadable",
      answered: 2,
      needed: 3,
    });
  });

  it("keeps any other error verbatim", async () => {
    expect(await balanceState(() => Promise.reject(new Error("no such function")))).toEqual({
      state: "failed",
      message: "no such function",
    });
  });
});
