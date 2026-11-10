// Profile files from earlier releases (`compat/fixtures/<release>/`) still load: a profile and a vault record each read
// at the current schema, migrated if older. Until launch (`compat/MODE` is `report`) a problem is printed, not failed.
// `OME_UPDATE_FIXTURES=<release>` writes this release's files.
import { copyFile, mkdir, mkdtemp, readFile, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { VAULT_RECORD_SCHEMA } from "../vaults/service.ts";
import { readVersioned, writeVersioned } from "./files.ts";
import type { Schema } from "./schema.ts";
import { DEFAULT_SETTINGS, PROFILE_SCHEMA } from "./state.ts";

const COMPAT = join(import.meta.dirname, "..", "..", "..", "..", "compat");
const HEX = (byte: string) => `0x${byte.repeat(32)}`;

/** What a profile and a vault record hold now: the files a release writes, kept as fixtures. */
const PROFILE = {
  profile: { name: "Ada", account: HEX("ad"), signerKey: { x: HEX("01"), y: HEX("02") } },
  settings: DEFAULT_SETTINGS,
  aliases: { [HEX("0a")]: "treasury" },
  defaultVault: HEX("0a"),
  tokens: {},
  contracts: {},
  apps: { "https://swap.example": { vault: HEX("0a"), name: "Swap", allowedAt: 1_900_000_000 } },
};
const VAULT_RECORD = {
  address: HEX("0a"),
  name: "Treasury",
  foundedAt: 1_900_000_000,
  founding: { lobby: null, session: HEX("5e"), deployTx: null },
  deployment: {
    salt: HEX("53"),
    signerKeys: [{ x: HEX("01"), y: HEX("02") }],
    threshold: 1,
    viewingThreshold: 2,
    groupKey: { x: HEX("03"), y: HEX("04") },
    verificationShares: [],
  },
  names: {},
  transcripts: {},
  config: null,
  status: "active",
};

const enforced = async () => (await readFile(join(COMPAT, "MODE"), "utf8")).trim() === "enforce";

const FILES = [
  ["profile.json", PROFILE_SCHEMA],
  ["vault.json", VAULT_RECORD_SCHEMA],
] as const;

/** What is wrong with `release`'s `file`, read through this release from a copy; `null` when it loads. */
async function problemWith(release: string, file: string, schema: Schema): Promise<string | null> {
  const copy = join(await mkdtemp(join(tmpdir(), "ome-fixture-")), file);
  try {
    await copyFile(join(COMPAT, "fixtures", release, file), copy);
  } catch {
    return null;
  }
  try {
    const value = await readVersioned<Record<string, unknown>>(copy, schema, {});
    return value.schema === schema.migrations.length ? null : `${release}/${file}: not upgraded`;
  } catch (error) {
    return `${release}/${file}: ${(error as Error).message}`;
  }
}

describe("profile files from earlier releases", () => {
  it("still load at the current schema", async () => {
    const writing = process.env.OME_UPDATE_FIXTURES;
    if (writing) {
      await mkdir(join(COMPAT, "fixtures", writing), { recursive: true });
      await writeVersioned(
        join(COMPAT, "fixtures", writing, "profile.json"),
        PROFILE_SCHEMA,
        PROFILE,
      );
      await writeVersioned(
        join(COMPAT, "fixtures", writing, "vault.json"),
        VAULT_RECORD_SCHEMA,
        VAULT_RECORD,
      );
    }
    const releases = await readdir(join(COMPAT, "fixtures")).catch(() => []);
    const problems = (
      await Promise.all(
        releases.flatMap((release) =>
          FILES.map(([file, schema]) => problemWith(release, file, schema)),
        ),
      )
    ).filter((problem): problem is string => problem !== null);
    if (problems.length > 0 && (await enforced())) expect(problems).toEqual([]);
    if (problems.length > 0)
      process.stderr.write(`compat (report mode, v0 still open):\n${problems.join("\n")}\n`);
  });
});
