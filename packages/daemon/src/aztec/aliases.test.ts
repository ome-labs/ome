// aztec-wallet's alias store, read and written as aztec-wallet itself does (`@aztec-labs/cli-wallet` WalletDB).
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { openStoreAt } from "@aztec-labs/kv-store/lmdb-v2";
import { afterEach, describe, expect, it } from "vitest";

import { listAliases, setAlias } from "./aliases.ts";

describe("aztec-wallet aliases", () => {
  let dir: string;
  afterEach(async () => rm(dir, { recursive: true, force: true }));

  it("reads what aztec-wallet wrote and writes what it reads", async () => {
    dir = await mkdtemp(join(tmpdir(), "ome-aztec-wallet-"));
    expect(await listAliases(join(dir, "missing"))).toEqual([]);
    await expect(
      setAlias(join(dir, "missing"), { key: "accounts:x", value: "0x01" }),
    ).rejects.toThrow(/no store/u);
    const store = await openStoreAt(dir);
    await store.openMap<string, Buffer>("aliases").set("accounts:main", Buffer.from("0x01"));
    await store.close();
    await setAlias(dir, { key: "accounts:treasury", value: "0x02" });
    expect(await listAliases(dir)).toEqual([
      { key: "accounts:main", value: "0x01" },
      { key: "accounts:treasury", value: "0x02" },
    ]);
    await expect(setAlias(dir, { key: "treasury", value: "0x02" })).rejects.toThrow(/type/u);
  });
});
