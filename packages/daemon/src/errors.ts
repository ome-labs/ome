import { ERROR_CODES, type ErrorKind, type RpcError } from "@ome-labs/api";

/** A failure the daemon reports with an API error kind. */
export class DaemonError extends Error {
  readonly kind: ErrorKind;

  constructor(kind: ErrorKind, message: string) {
    super(message);
    this.name = "DaemonError";
    this.kind = kind;
  }
}

const KINDS = new Set(Object.keys(ERROR_CODES));

/**
 * The JSON-RPC error for anything a handler threw. Errors from the keeper (`KeeperError`) and the bindings
 * (`OmeWasmError`) keep their kinds; anything else is `failed`.
 */
export function toRpcError(error: unknown): RpcError {
  const message = error instanceof Error ? error.message : String(error);
  const given = (error as { kind?: unknown } | null)?.kind;
  const kind = typeof given === "string" && KINDS.has(given) ? (given as ErrorKind) : "failed";
  return { code: ERROR_CODES[kind], message, data: { kind } };
}

/** Throws `invalid` unless `condition` holds. */
export function require(condition: unknown, message: string): asserts condition {
  if (!condition) throw new DaemonError("invalid", message);
}
