#!/usr/bin/env node
// The previous release and this build in one vault: the CLI scenario (`scripts/test-cli.ts`) with Alice on the latest
// published `@ome-labs/ome` and Bob and Carol on this build. Until launch (`compat/MODE` is `report`) a failure is
// reported and the lane passes, since protocol v0 is still open; from launch it fails.
//
//   node scripts/test-compat.ts          (via `pnpm test:compat`, which builds this build first)

import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const enforce = readFileSync(join(root, "compat", "MODE"), "utf8").trim() === "enforce";

const previous = execFileSync("npm", ["view", "@ome-labs/ome", "version"], {
  encoding: "utf8",
}).trim();
const prefix = mkdtempSync(join(tmpdir(), "ome-previous-"));
console.log(`installing @ome-labs/ome@${previous} for Alice`);
execFileSync("npm", ["install", "--prefix", prefix, `@ome-labs/ome@${previous}`], {
  stdio: "inherit",
});

// Releases that can't work together fail a step rather than wait on it: each test gets two minutes, the run fifteen.
const result = spawnSync(
  "node",
  ["--test", "--test-timeout=120000", join(root, "scripts", "test-cli.ts")],
  {
    cwd: root,
    stdio: "inherit",
    timeout: 15 * 60 * 1000,
    killSignal: "SIGKILL",
    env: { ...process.env, OME_PREVIOUS_BIN: join(prefix, "node_modules", ".bin", "ome") },
  },
);
// Whatever the previous release left running (its daemon, keeper and prover) goes with the lane.
spawnSync("pkill", ["-9", "-f", prefix]);
if (result.status === 0) {
  console.log(`compat: ${previous} and this build work together`);
  process.exit(0);
}
if (enforce) process.exit(result.status ?? 1);
console.log(
  `compat (report mode, v0 still open): ${previous} and this build don't work together yet`,
);
