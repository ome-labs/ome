// Compile-time checks that the generated types say what architecture/api.md promises. `tsc` fails if one breaks.
import type {
  DaemonEvent,
  DaemonParams,
  DaemonResult,
  KeeperEvent,
  KeeperParams,
  KeeperResult,
  RpcError,
} from "../src/index.ts";
import { API_VERSION, ERROR_CODES } from "../src/index.ts";

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
const check = <T extends true>(): T => true as T;

// Missing values are null in results, and optional params may be omitted.
check<Equal<KeeperResult<"proposals.get">, string | null>>();
check<Equal<KeeperParams<"audit.list">, { vault: string; limit?: number }>>();
const approve: DaemonParams<"proposals.approve"> = { vault: "0x00", id: "0x00" };

// Amounts are decimal strings and times are numbers.
const balance: DaemonResult<"tokens.list">[number]["private"] = {
  state: "known",
  amount: "25000000000000000000",
};
const seen: KeeperResult<"peers.status">[number]["lastSeen"] = 1_800_000_000;

// Events are tagged unions.
const onKeeper = (event: KeeperEvent): string =>
  event.type === "unexpectedSpend" ? event.txHash : event.type;
const onDaemon = (event: DaemonEvent): string =>
  event.type === "keeperEvent" ? onKeeper(event.event) : event.type;

// Errors carry a kind.
const locked: RpcError = { code: ERROR_CODES.locked, message: "locked", data: { kind: "locked" } };

export const used = [approve, balance, seen, onDaemon, locked, API_VERSION] as const;
