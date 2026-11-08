#!/usr/bin/env node
// Writes a gate-count flamegraph for every function of the given contract artifacts into
// `<target-dir>/flamegraphs/<artifact>/<function>.svg`, and prints each function's gate count.
//
// Usage: node scripts/noir-flamegraphs.ts <target-dir> <artifact-name>...
// Example: node scripts/noir-flamegraphs.ts target ome_vault-OmeVault

import { execFileSync } from "node:child_process";
import { mkdirSync, renameSync } from "node:fs";
import { join } from "node:path";

const [targetDir, ...artifacts] = process.argv.slice(2);
if (!targetDir || artifacts.length === 0) {
  console.error("usage: noir-flamegraphs.ts <target-dir> <artifact-name>...");
  process.exit(2);
}

/** One row of `aztec profile gates --json`. */
interface GateReport {
  name: string;
  type: string;
  gates: number;
}

const report = JSON.parse(
  execFileSync("aztec", ["profile", "gates", targetDir, "--json"], { encoding: "utf8" }),
) as GateReport[];

for (const artifact of artifacts) {
  const functions = report
    .filter(({ name, type }) => type === "contract-function" && name.startsWith(`${artifact}::`))
    .map(({ name, gates }) => ({ name: name.slice(artifact.length + 2), gates }));

  if (functions.length === 0) {
    console.error(
      `no compiled functions found for ${artifact} in ${targetDir}; run the build first`,
    );
    process.exit(1);
  }

  const outputDir = join(targetDir, "flamegraphs", artifact);
  mkdirSync(outputDir, { recursive: true });

  for (const { name, gates } of functions) {
    execFileSync("aztec", ["profile", "flamegraph", join(targetDir, `${artifact}.json`), name], {
      stdio: "ignore",
    });

    // `aztec profile flamegraph` always writes beside the artifact; collect the files in one folder per artifact.
    const written = join(targetDir, `${artifact}-${name}-flamegraph.svg`);
    const destination = join(outputDir, `${name}.svg`);
    renameSync(written, destination);

    console.log(`${String(gates.toLocaleString("en-US")).padStart(8)} gates  ${destination}`);
  }
}
