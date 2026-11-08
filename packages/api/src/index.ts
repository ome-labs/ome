// The Ome keeper and daemon APIs (architecture/api.md). Every shape is generated from crates/api; this file only adds
// the lookups a client needs to type a call.
import type { DaemonMethods, KeeperMethods } from "./generated/api.ts";

export * from "./generated/api.ts";

/** A keeper method name. */
export type KeeperMethod = keyof KeeperMethods;

/** A daemon method name. */
export type DaemonMethod = keyof DaemonMethods;

/** Params of keeper method `M`. */
export type KeeperParams<M extends KeeperMethod> = KeeperMethods[M]["params"];

/** Result of keeper method `M`. */
export type KeeperResult<M extends KeeperMethod> = KeeperMethods[M]["result"];

/** Params of daemon method `M`. */
export type DaemonParams<M extends DaemonMethod> = DaemonMethods[M]["params"];

/** Result of daemon method `M`. */
export type DaemonResult<M extends DaemonMethod> = DaemonMethods[M]["result"];
