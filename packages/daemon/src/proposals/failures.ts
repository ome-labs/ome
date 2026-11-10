// Review and simulation failures as kinds a client acts on, from the errors the wallet, the bindings and Aztec throw.
// A failure that matches no rule stays `invalid` or `reverted` with its own message: nothing here guesses.
import { SimulationError as AztecSimulationError } from "@aztec-labs/stdlib/errors";
import type { Hex32, ReviewError, SimulationError, TokenInfo } from "@ome-labs/api";

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));
const lower = (value: { toString(): string }) => value.toString().toLowerCase() as Hex32;

interface Classified {
  readonly kind?: unknown;
  readonly reason?: unknown;
  readonly details?: {
    readonly made?: number;
    readonly current?: number;
    readonly at?: number;
    readonly classId?: Hex32;
  };
}

/** Why rebuilding and checking a proposal on this device failed. */
export function reviewError(error: unknown): ReviewError {
  const text = message(error);
  const { kind, reason, details } = (error ?? {}) as Classified;
  if (
    reason === "outer-hash" ||
    /does not hash to the proposed outer hash|payload does not match/u.test(text)
  )
    return { failure: { kind: "payloadMismatch" }, message: text };
  if (kind === "stale" && details?.made !== undefined && details.current !== undefined)
    return {
      failure: { kind: "configChanged", made: details.made, current: details.current },
      message: text,
    };
  if (kind === "expired" && details?.at !== undefined)
    return { failure: { kind: "expired", at: details.at }, message: text };
  if (kind === "unknown-contract")
    return {
      failure: { kind: "artifactMissing", classId: details?.classId ?? null },
      message: text,
    };
  if (/no member in slot/u.test(text)) return { failure: { kind: "notMember" }, message: text };
  if (/pays fees from the vault in public/u.test(text))
    return { failure: { kind: "feeOnVault" }, message: text };
  return { failure: { kind: "invalid" }, message: text };
}

/** The contract and function a simulation failed in, from Aztec's call stack. */
function failingCall(error: unknown): { contract: Hex32 | null; fn: string | null } {
  if (!(error instanceof AztecSimulationError)) return { contract: null, fn: null };
  const failing = error.getCallStack().at(-1);
  return {
    contract: failing ? lower(failing.contractAddress) : null,
    fn: failing?.functionName ?? null,
  };
}

/** Why a simulation failed. `tokens` names the contract that ran short, when the vault tracks it. */
export function simulationError(error: unknown, tokens: readonly TokenInfo[]): SimulationError {
  const text = message(error);
  const original = error instanceof AztecSimulationError ? error.getOriginalMessage() : text;
  const { contract, fn } = failingCall(error);
  if (/balance too low/iu.test(original) && contract)
    return {
      failure: {
        kind: "insufficientBalance",
        contract,
        token: tokens.find((t) => t.address === contract) ?? null,
      },
      message: "the vault doesn't hold enough of this token",
    };
  if (/message not authorized by account|unknown auth witness/iu.test(original))
    return {
      failure: { kind: "authwitRequired", contract },
      message: "a contract asked the vault for an authwit this proposal doesn't carry",
    };
  if (/nullifier collision|duplicate nullifier|nullifier already exists/iu.test(original))
    return {
      failure: { kind: "nonceUsed" },
      message: "the proposal's nonce is spent: it was executed or cancelled",
    };
  const { kind } = (error ?? {}) as Classified;
  if (
    kind === "not-ready" ||
    kind === "unreadable" ||
    /unreachable|ECONNREFUSED|fetch failed|members answered/iu.test(text)
  )
    return { failure: { kind: "unavailable" }, message: text };
  return { failure: { kind: "reverted", contract, function: fn }, message: original };
}
