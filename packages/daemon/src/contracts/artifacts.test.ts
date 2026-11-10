// Artifacts travel between members as gzip chunks under the wire's limit; whoever sent one isn't trusted for it, so it
// is refused unless it computes to the class it was fetched for.
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import type { Hex32 } from "@ome-labs/api";
import { describe, expect, it } from "vitest";

import { CHUNK_BYTES, classIdOf, pack, unpack } from "./artifacts.ts";

describe("artifacts", () => {
  it("pack into chunks under the limit and unpack to the same class", async () => {
    const classId = await classIdOf(TokenContract.artifact);
    const chunks = pack(TokenContract.artifact);
    expect(chunks.length).toBeGreaterThan(1);
    expect(chunks.every((chunk) => chunk.length <= CHUNK_BYTES)).toBe(true);
    const artifact = await unpack(chunks, classId);
    expect(await classIdOf(artifact)).toBe(classId);
  });

  it("refuse an artifact that isn't the code of the class it claims", async () => {
    const other = `0x${"11".repeat(32)}` as Hex32;
    await expect(unpack(pack(TokenContract.artifact), other)).rejects.toThrow(
      "is the code of class",
    );
    await expect(unpack([Buffer.from("not gzip")], other)).rejects.toThrow("doesn't decode");
  });
});
