#!/usr/bin/env node
// Freezes what a release must keep reading (`compat/README.md`), before launch only:
//
//   node scripts/compat-freeze.ts <release>
//
// copies the current wire vectors to compat/vectors/v<N>.json (N, the protocol version they encode) and writes the
// release's fixtures (a keeper store from a real three-keeper vault, a profile and a vault record) to
// compat/fixtures/<release>/. After launch (`compat/MODE` is `enforce`) a frozen version never changes: it refuses.

import { execFileSync } from "node:child_process";
import { copyFileSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const release = process.argv[2];
if (!release || !/^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$/u.test(release)) {
  console.error("usage: node scripts/compat-freeze.ts <release>, e.g. 0.3.0");
  process.exit(2);
}
if (readFileSync(join(root, "compat", "MODE"), "utf8").trim() === "enforce") {
  console.error(
    "compat/MODE is enforce: frozen versions don't change after launch; add a protocol version",
  );
  process.exit(1);
}

const vectors = join(root, "crates", "wire", "tests", "vectors", "wire.json");
const { protocolVersion } = JSON.parse(readFileSync(vectors, "utf8")) as {
  protocolVersion: number;
};
copyFileSync(vectors, join(root, "compat", "vectors", `v${protocolVersion}.json`));
console.log(`froze protocol v${protocolVersion}`);

const env = { ...process.env, OME_UPDATE_FIXTURES: release };
const run = (command: string, args: string[]) =>
  execFileSync(command, args, { cwd: root, stdio: "inherit", env });
run("cargo", [
  "test",
  "--manifest-path",
  join(root, "crates", "Cargo.toml"),
  "-p",
  "ome-keeper",
  "--test",
  "fixtures",
]);
run("pnpm", [
  "--filter",
  "@ome-labs/daemon",
  "exec",
  "vitest",
  "run",
  "src/profile/fixtures.test.ts",
]);
console.log(`wrote compat/fixtures/${release}`);
