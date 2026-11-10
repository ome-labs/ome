import {
  ERROR_CODES,
  type ErrorDetails,
  type ErrorKind,
  type NotReadyReason,
  type RpcError,
} from "@ome-labs/api";

/** A failure the daemon reports with an API error kind, and the numbers to fix it when the kind has them. */
export class DaemonError extends Error {
  readonly kind: ErrorKind;
  readonly details: ErrorDetails | undefined;

  constructor(kind: ErrorKind, message: string, details?: ErrorDetails) {
    super(message);
    this.name = "DaemonError";
    this.kind = kind;
    this.details = details;
  }

  /** A failure whose kind is the one `details` belong to. */
  static detailed(details: ErrorDetails, message: string): DaemonError {
    return new DaemonError(details.kind, message, details);
  }
}

/** `not-ready`, naming what it waits for. */
export function notReady(reason: NotReadyReason, message: string): DaemonError {
  return DaemonError.detailed({ kind: "not-ready", reason }, message);
}

const KINDS = new Set(Object.keys(ERROR_CODES));
/** How far down an error's `cause` chain a kind is looked for: Aztec and the PXE wrap what they rethrow. */
const CAUSE_DEPTH = 5;

interface Classified {
  readonly kind?: unknown;
  readonly details?: unknown;
  readonly cause?: unknown;
}

/** The first error in the cause chain that carries an API kind, with its details when they match that kind. */
function classify(error: unknown): { kind: ErrorKind; details: ErrorDetails | undefined } {
  let current: unknown = error;
  for (let depth = 0; depth < CAUSE_DEPTH && current; depth += 1) {
    const { kind, details, cause } = current as Classified;
    if (typeof kind === "string" && KINDS.has(kind)) {
      const matching =
        typeof details === "object" && details !== null && (details as ErrorDetails).kind === kind;
      return { kind: kind as ErrorKind, details: matching ? (details as ErrorDetails) : undefined };
    }
    current = cause;
  }
  return { kind: "failed", details: undefined };
}

/**
 * The JSON-RPC error for anything a handler threw. Errors from the keeper (`KeeperError`) and the bindings
 * (`OmeWasmError`) keep their kinds and details, also when wrapped; anything else is `failed`.
 */
export function toRpcError(error: unknown): RpcError {
  const message = error instanceof Error ? error.message : String(error);
  const { kind, details } = classify(error);
  return { code: ERROR_CODES[kind], message, data: details ? { kind, details } : { kind } };
}

/** Throws `invalid` unless `condition` holds. */
export function require(condition: unknown, message: string): asserts condition {
  if (!condition) throw new DaemonError("invalid", message);
}
