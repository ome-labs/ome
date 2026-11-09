#!/usr/bin/env node
// Runs the suites that need `aztec start --local-network`, in order, with one summary:
//
//   node scripts/e2e.ts e2e        the wallet and the daemons end to end (founding, paying, rotating, phone, server keeper)
//   node scripts/e2e.ts removal    the member removal drill
//   node scripts/e2e.ts cli        three members through `ome` only (founding, paying, removing a member)
//
// It builds what the suites run (the contract artifact, @ome-labs/wasm, ome, ome-keeper, ome-relay, the test phone), checks
// the local network at AZTEC_NODE_URL (default http://localhost:8080), and with `--start` starts it when it is not
// running (Docker must be up) and stops it afterwards.

import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { openSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const node = process.env.AZTEC_NODE_URL ?? "http://localhost:8080";

/** Each suite's parts, as the command that runs each. */
const SUITES: Record<string, { name: string; command: string[] }[]> = {
  e2e: [
    { name: "wallet", command: ["pnpm", "--filter", "@ome-labs/wallet", "test:e2e"] },
    { name: "daemons", command: ["pnpm", "--filter", "@ome-labs/daemon", "test:e2e"] },
  ],
  removal: [
    { name: "removal drill", command: ["pnpm", "--filter", "@ome-labs/daemon", "test:removal"] },
  ],
  cli: [{ name: "ome cli", command: ["node", "--test", join(root, "scripts", "test-cli.ts")] }],
};

const args = process.argv.slice(2);
const suite = SUITES[args.find((a) => !a.startsWith("--")) ?? ""];
if (!suite) {
  console.error(`usage: node scripts/e2e.ts ${Object.keys(SUITES).join("|")} [--start]`);
  process.exit(2);
}

function run(command: string, commandArgs: string[]): void {
  execFileSync(command, commandArgs, { cwd: root, stdio: "inherit" });
}

async function networkUp(): Promise<boolean> {
  try {
    const response = await fetch(node, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "node_getNodeInfo", params: [] }),
      signal: AbortSignal.timeout(3000),
    });
    return response.ok && "result" in ((await response.json()) as object);
  } catch {
    return false;
  }
}

async function startNetwork(): Promise<ChildProcess> {
  const log = join(tmpdir(), "ome-local-network.log");
  console.log(`starting aztec start --local-network (log: ${log})`);
  const child = spawn("aztec", ["start", "--local-network"], {
    stdio: ["ignore", openSync(log, "a"), openSync(log, "a")],
  });
  for (let i = 0; i < 150; i += 1) {
    // oxlint-disable-next-line no-await-in-loop
    if (await networkUp()) return child;
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, 2000));
  }
  child.kill();
  throw new Error("the local network did not start within five minutes");
}

let started: ChildProcess | undefined;
if (!(await networkUp())) {
  if (!args.includes("--start")) {
    console.error(`no Aztec node at ${node}: run \`aztec start --local-network\`, or pass --start`);
    process.exit(1);
  }
  started = await startNetwork();
}

const results: { name: string; ok: boolean; seconds: number }[] = [];
try {
  run("pnpm", [
    "exec",
    "turbo",
    "run",
    "build",
    "--filter=@ome-labs/contracts",
    "--filter=@ome-labs/wasm",
  ]);
  run("cargo", [
    "build",
    "--manifest-path",
    join(root, "crates", "Cargo.toml"),
    "-p",
    "ome-cli",
    "-p",
    "ome-keeper",
    "-p",
    "ome-relay",
    "-p",
    "ome-ffi",
    "--features",
    "ome-ffi/cli",
    "--bins",
  ]);
  for (const { name, command } of suite) {
    const start = Date.now();
    let ok = true;
    try {
      const [program = "", ...rest] = command;
      run(program, rest);
    } catch {
      ok = false;
    }
    results.push({ name, ok, seconds: Math.round((Date.now() - start) / 1000) });
  }
} finally {
  started?.kill();
}

console.log("\nsummary");
for (const { name, ok, seconds } of results)
  console.log(`  ${ok ? "pass" : "FAIL"}  ${name.padEnd(16)} ${seconds} s`);
process.exit(results.every((r) => r.ok) ? 0 : 1);
