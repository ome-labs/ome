import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { openStoreAt } from "@aztec-labs/kv-store/lmdb-v2";
import type { AztecAlias } from "@ome-labs/api";

import { DaemonError } from "../errors.ts";

const KEY = /^(accounts|contracts|artifacts|secrets|transactions|authwits):[^\s]{1,64}$/u;

/** `aztec-wallet`'s data directory: the configured one, or its default `~/.aztec/wallet`. */
export const walletDir = (configured: string | null): string =>
  configured && configured.length > 0 ? configured : join(homedir(), ".aztec", "wallet");

/**
 * `aztec-wallet`'s typed aliases (`accounts:alice`, `contracts:usdc`, `artifacts:token`), read from its own store
 * (`@aztec-labs/cli-wallet` keeps them in the `aliases` map of an LMDB store in its data directory). The store is
 * opened for the call only, so `aztec-wallet` keeps working beside it. No store yet means no aliases.
 */
export async function listAliases(dir: string): Promise<AztecAlias[]> {
  if (!hasStore(dir)) return [];
  const store = await openStoreAt(dir);
  try {
    const out: AztecAlias[] = [];
    for await (const [key, value] of store.openMap<string, Buffer>("aliases").entriesAsync()) {
      out.push({ key, value: Buffer.from(value).toString("utf8") });
    }
    return out;
  } finally {
    await store.close();
  }
}

/** Writes one alias into `aztec-wallet`'s store, as `aztec-wallet alias <type> <name> <value>` would. */
export async function setAlias(dir: string, alias: AztecAlias): Promise<void> {
  if (!KEY.test(alias.key))
    throw new DaemonError("invalid", "an aztec-wallet alias is <type>:<name>");
  if (!hasStore(dir))
    throw new DaemonError(
      "not-found",
      `aztec-wallet has no store at ${dir}; run aztec-wallet once`,
    );
  const store = await openStoreAt(dir);
  try {
    await store.openMap<string, Buffer>("aliases").set(alias.key, Buffer.from(alias.value, "utf8"));
  } finally {
    await store.close();
  }
}

/**
 * Whether `dir` holds an LMDB store. Opening a missing directory aborts the whole process (the native binding throws
 * an uncaught C++ exception), so this is checked first.
 */
function hasStore(dir: string): boolean {
  return existsSync(join(dir, "data.mdb"));
}
