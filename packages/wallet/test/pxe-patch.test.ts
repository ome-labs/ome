// The wallet's PXE comes from @ome-labs/pxe, Aztec's PXE with Ome's patch: every site that used the incoming viewing
// secret asks the key store first. Fails loudly after an Aztec upgrade until the patch is ported.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

// `@ome-labs/pxe/server` resolves to `dest/entrypoints/server/index.js`.
const dest = fileURLToPath(new URL("../../", import.meta.resolve("@ome-labs/pxe/server")));
const read = (path: string) => readFile(`${dest}${path}`, "utf8");

const sites: [string, number][] = [
  ["storage/open_pxe_stores.js", 1],
  ["pxe.js", 1],
  ["contract_function_simulator/oracle/utility_execution_oracle.js", 1],
  ["contract_function_simulator/oracle/private_execution_oracle.js", 2],
  ["logs/log_service.js", 1],
];

describe("@ome-labs/pxe", () => {
  it.each(sites)("patches %s", async (path, markers) => {
    const source = await read(path);
    expect(source.match(/\/\/ ome:/g)?.length).toBe(markers);
  });

  it("passes the key store factory from createPXE to PXE.create", async () => {
    expect(await read("entrypoints/server/utils.js")).toContain(
      "createKeyStore: options.createKeyStore",
    );
  });

  it("leaves no raw incoming viewing secret read outside a threshold guard", async () => {
    const sources = await Promise.all(
      sites.map(async ([path]) => [path, await read(path)] as const),
    );
    for (const [path, source] of sources) {
      // Every read sits in a conditional whose guard (a few lines up in the same statement) checks for threshold accounts.
      for (const match of source.matchAll(/getMasterIncomingViewingSecretKey\(/g)) {
        const statement = source.slice(Math.max(0, match.index - 600), match.index);
        expect(statement, `${path} at ${match.index}`).toMatch(/isThresholdAccount/);
      }
    }
  });
});
