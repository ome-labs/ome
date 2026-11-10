// The keeper's master key, when a sealed file holds it, moves with the passphrase: re-sealed under the new sealing key,
// it opens with that key and no longer with the old one.
import { randomBytes } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { masterKeyStore } from "./master-key.ts";

describe("sealed master key", () => {
  let dir: string;
  const previous = process.env.OME_KEYCHAIN;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), "ome-master-"));
    process.env.OME_KEYCHAIN = "off";
  });

  afterEach(async () => {
    process.env.OME_KEYCHAIN = previous;
    await rm(dir, { recursive: true, force: true });
  });

  it("re-seals under the new sealing key and keeps the same key", async () => {
    const path = join(dir, "keeper-key.sealed");
    const store = await masterKeyStore("test", path);
    expect(store.kind).toBe("file");
    const [current, next] = [randomBytes(32), randomBytes(32)];
    const key = await store.create(current);

    const resealed = await store.resealed(current, next);
    if (!resealed) throw new Error("a sealed file is re-sealed");
    await writeFile(path, resealed);

    expect(await store.read(next)).toEqual(key);
    await expect(store.read(current)).rejects.toThrow();
  });

  it("has nothing to re-seal before the keeper first made its key", async () => {
    const store = await masterKeyStore("test", join(dir, "keeper-key.sealed"));
    expect(await store.resealed(randomBytes(32), randomBytes(32))).toBeNull();
  });
});
