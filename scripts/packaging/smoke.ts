#!/usr/bin/env node
// Checks an installed Ome: `ome` on PATH reports the version and install method, finds its daemon and keeper, and
// the daemon starts in a throwaway home.
//
//   node scripts/packaging/smoke.ts <version> <install-method>     (method as `ome doctor` names it: npm, Homebrew, …)
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { args } from "./targets.ts";

const [version, method] = args("smoke.ts", ["version", "install-method"]);
const home = mkdtempSync(join(tmpdir(), "ome-smoke-"));
const env = { ...process.env, OME_HOME: home };

function run(program: string, argv: string[]): string {
  try {
    return execFileSync(program, argv, {
      env,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "inherit"],
    });
  } catch (error) {
    // `ome doctor` exits non-zero for the missing profile; its JSON is still on stdout.
    const { stdout } = error as { stdout?: string };
    if (stdout) {
      return stdout;
    }
    throw error;
  }
}

function expect(condition: boolean, what: string): void {
  if (!condition) {
    throw new Error(`smoke: ${what}`);
  }
  process.stdout.write(`ok  ${what}\n`);
}

try {
  expect(run("ome", ["--version"]).trim() === `ome ${version}`, `ome --version is ${version}`);
  expect(
    run("ome-keeper", ["--version"]).trim() === `ome-keeper ${version}`,
    `ome-keeper --version is ${version}`,
  );

  const checks = JSON.parse(run("ome", ["doctor", "--json"])) as { check: string; ok: boolean }[];
  const passed = (prefix: string) => checks.some((c) => c.ok && c.check.startsWith(prefix));
  expect(passed(`ome ${version} installed by ${method}`), `doctor: installed by ${method}`);
  expect(passed("omed at "), "doctor: omed found");
  expect(passed("ome-keeper at "), "doctor: ome-keeper found");

  // `ome daemon start` returns once the daemon answers; no Aztec node is needed for that.
  run("ome", ["daemon", "start", "--profile", "smoke"]);
  expect(existsSync(join(home, "profiles", "smoke", "daemon.sock")), "omed starts");
  run("ome", ["daemon", "stop", "--profile", "smoke"]);
} finally {
  rmSync(home, { recursive: true, force: true });
}
