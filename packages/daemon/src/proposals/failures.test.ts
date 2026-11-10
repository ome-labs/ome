// Failures read as kinds a client acts on: the token's "Balance too low" as insufficient balance (with the token when
// the vault tracks it), a missing authwit, a spent nonce; rebuild failures by the wallet's and protocol's own kinds.
// Anything else keeps its message, as `reverted` or `invalid`.
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { SimulationError } from "@aztec-labs/stdlib/errors";
import type { Hex32, TokenInfo } from "@ome-labs/api";
import { VaultTxError } from "@ome-labs/wallet";
import { describe, expect, it } from "vitest";

import { DaemonError } from "../errors.ts";
import { reviewError, simulationError } from "./failures.ts";

const TOKEN = `0x${"0c".repeat(32)}` as Hex32;
const OMET: TokenInfo = { address: TOKEN, name: "Ome Test", symbol: "OMET", decimals: 18 };

const failedIn = (message: string, functionName = "transfer") =>
  new SimulationError(message, [
    { contractAddress: AztecAddress.fromStringUnsafe(TOKEN), functionName },
  ]);

describe("simulationError", () => {
  it("names a token the vault holds too little of", () => {
    expect(simulationError(failedIn("Assertion failed: Balance too low"), [OMET])).toMatchObject({
      failure: { kind: "insufficientBalance", contract: TOKEN, token: OMET },
    });
    expect(simulationError(failedIn("Balance too low"), [])).toMatchObject({
      failure: { kind: "insufficientBalance", token: null },
    });
  });

  it("recognises a missing authwit and a spent nonce", () => {
    expect(simulationError(failedIn("Message not authorized by account"), []).failure).toEqual({
      kind: "authwitRequired",
      contract: TOKEN,
    });
    expect(simulationError(new Error("Nullifier collision"), []).failure.kind).toBe("nonceUsed");
  });

  it("keeps anything else as a revert with its own message, or unavailable when nothing could run", () => {
    expect(simulationError(failedIn("Assertion failed: not the admin", "mint"), [])).toEqual({
      failure: { kind: "reverted", contract: TOKEN, function: "mint" },
      message: "Assertion failed: not the admin",
    });
    expect(
      simulationError(new DaemonError("unreadable", "only 1 of 2 members answered"), []).failure
        .kind,
    ).toBe("unavailable");
  });
});

describe("reviewError", () => {
  it("reads the wallet's and the protocol's kinds", () => {
    expect(reviewError(new VaultTxError("outer-hash", "hashes differently")).failure).toEqual({
      kind: "payloadMismatch",
    });
    expect(
      reviewError(new VaultTxError("stale-config", "stale", { kind: "stale", made: 1, current: 2 }))
        .failure,
    ).toEqual({ kind: "configChanged", made: 1, current: 2 });
    expect(
      reviewError(
        Object.assign(new Error("deadline"), {
          kind: "expired",
          details: { kind: "expired", at: 9 },
        }),
      ).failure,
    ).toEqual({
      kind: "expired",
      at: 9,
    });
    expect(reviewError(new Error("something odd"))).toEqual({
      failure: { kind: "invalid" },
      message: "something odd",
    });
  });
});
