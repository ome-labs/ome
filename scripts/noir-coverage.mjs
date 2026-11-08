#!/usr/bin/env node
// Summarises a Nargo LCOV report for one contract package: line and function coverage per source file, plus the
// lines no test reached. Nargo writes one record per test for every file in the dependency graph, so records are
// merged and limited to the package's own sources, excluding its tests.
//
// Usage: node scripts/noir-coverage.mjs <lcov.info> <package-dir>
// Example: node scripts/noir-coverage.mjs contracts/target/coverage/ome_vault/lcov.info ome_vault

import { readFileSync } from "node:fs";

const [lcovPath, packageDir] = process.argv.slice(2);
if (!lcovPath || !packageDir) {
  console.error("usage: noir-coverage.mjs <lcov.info> <package-dir>");
  process.exit(2);
}

const sourcePrefix = `${packageDir}/src/`;
const testPrefix = `${packageDir}/src/test/`;

/** @type {Map<string, { lines: Map<number, number>, functions: Map<string, number> }>} */
const files = new Map();
let current = null;

for (const line of readFileSync(lcovPath, "utf8").split("\n")) {
  if (line.startsWith("SF:")) {
    const path = line.slice(3);
    const isOwnSource = path.startsWith(sourcePrefix) && !path.startsWith(testPrefix);
    current = isOwnSource ? (files.get(path) ?? { lines: new Map(), functions: new Map() }) : null;
    if (current) files.set(path, current);
  } else if (current && line.startsWith("DA:")) {
    const [number, hits] = line.slice(3).split(",").map(Number);
    current.lines.set(number, (current.lines.get(number) ?? 0) + hits);
  } else if (current && line.startsWith("FNDA:")) {
    const [hits, name] = line.slice(5).split(",");
    current.functions.set(name, (current.functions.get(name) ?? 0) + Number(hits));
  } else if (line === "end_of_record") {
    current = null;
  }
}

const percent = (hit, total) => (total === 0 ? "-" : `${((100 * hit) / total).toFixed(1)}%`);
const covered = (counts) => [...counts.values()].filter((hits) => hits > 0).length;

let linesHit = 0;
let linesTotal = 0;
const rows = [];

for (const [path, { lines, functions }] of [...files].toSorted(([a], [b]) => a.localeCompare(b))) {
  const hit = covered(lines);
  const missed = [...lines]
    .filter(([, hits]) => hits === 0)
    .map(([number]) => number)
    .toSorted((a, b) => a - b);
  linesHit += hit;
  linesTotal += lines.size;

  rows.push({
    file: path.slice(sourcePrefix.length),
    lines: `${hit}/${lines.size} ${percent(hit, lines.size)}`,
    functions: `${covered(functions)}/${functions.size}`,
    "lines never reached": missed.join(" "),
  });
}

console.table(rows);
console.log(`Total line coverage: ${linesHit}/${linesTotal} (${percent(linesHit, linesTotal)})`);
// Measured with Aztec 6.0.0: a TXE test is recorded only up to its first TXE oracle call (creating an account,
// deploying, calling a contract), so the report reflects tests that run without the TXE.
console.log(
  "Only tests that run without the TXE are measured: Nargo stops recording a test at its first TXE call. Code " +
    "reached only through the TXE (is_authorized, the contract functions in main.nr) is exercised by the " +
    "integration and end-to-end suites but reported as unreached.",
);
