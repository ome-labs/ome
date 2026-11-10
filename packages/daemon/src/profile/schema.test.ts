// Profile files upgrade themselves: a file from an older release is migrated step by step and written back with the
// old one kept beside it, a current one is left alone, and one a newer release wrote is refused.
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { readVersioned } from "./files.ts";
import { NewerFileError, type Schema, upgrade } from "./schema.ts";
import { PROFILE_SCHEMA } from "./state.ts";

const SCHEMA: Schema = {
  name: "test file",
  migrations: [
    (file) => ({ ...file, added: true }),
    ({ old, ...file }) => ({ ...file, renamed: old }),
  ],
};

describe("profile file schemas", () => {
  it("migrates a file step by step from the schema it was written at", () => {
    expect(upgrade(SCHEMA, { old: 1 }, "f")).toEqual({
      value: { added: true, renamed: 1, schema: 2 },
      from: 0,
      migrated: true,
    });
    expect(upgrade(SCHEMA, { added: true, old: 2, schema: 1 }, "f").value).toEqual({
      added: true,
      renamed: 2,
      schema: 2,
    });
    expect(upgrade(SCHEMA, { renamed: 3, schema: 2 }, "f").migrated).toBe(false);
    expect(() => upgrade(SCHEMA, { schema: 3 }, "f")).toThrow(NewerFileError);
    expect(() => upgrade(SCHEMA, { schema: 3 }, "f")).toThrow(/written by a newer Ome/u);
  });

  it("writes a migrated file back and keeps the old one", async () => {
    const dir = await mkdtemp(join(tmpdir(), "ome-schema-"));
    const path = join(dir, "profile.json");
    // A profile from before files had a schema, and before contracts and apps.
    const old = { profile: null, settings: {}, aliases: {}, defaultVault: null, tokens: {} };
    await writeFile(path, JSON.stringify(old));

    const file = await readVersioned<Record<string, unknown>>(path, PROFILE_SCHEMA, {});
    expect(file).toMatchObject({ contracts: {}, apps: {}, schema: 1 });
    expect(JSON.parse(await readFile(path, "utf8"))).toMatchObject({ schema: 1, apps: {} });
    expect(JSON.parse(await readFile(`${path}.bak-0`, "utf8"))).toEqual(old);
    expect(await readVersioned(join(dir, "missing.json"), PROFILE_SCHEMA, "fallback")).toBe(
      "fallback",
    );
  });
});
