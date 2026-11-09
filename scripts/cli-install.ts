#!/usr/bin/env node
// Installs the command line into the Ome home's bin directory (`$OME_HOME/bin`, by default `~/.ome/bin`):
//
//   ome          the command line (release build)
//   ome-keeper   the keeper the daemon runs (release build)
//   omed         the daemon: a wrapper that runs this checkout's packages/daemon/src/omed.ts with Node
//
// Then prints the line that puts them on PATH.

import { execFileSync } from "node:child_process";
import { chmod, copyFile, mkdir, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const home = process.env.OME_HOME || join(homedir(), ".ome");
const bin = join(home, "bin");

execFileSync(
  "cargo",
  [
    "build",
    "--release",
    "--manifest-path",
    join(root, "crates", "Cargo.toml"),
    "-p",
    "ome-cli",
    "-p",
    "ome-keeper",
  ],
  { stdio: "inherit" },
);
await mkdir(bin, { recursive: true });
for (const name of ["ome", "ome-keeper"]) {
  const target = join(bin, name);
  // oxlint-disable-next-line no-await-in-loop
  await copyFile(join(root, "crates", "target", "release", name), target);
  // oxlint-disable-next-line no-await-in-loop
  await chmod(target, 0o755);
}
const omed = join(bin, "omed");
await writeFile(
  omed,
  `#!/bin/sh\n# omed from ${root}\nexec node "${join(root, "packages", "daemon", "src", "omed.ts")}" "$@"\n`,
);
await chmod(omed, 0o755);

console.log(`\nInstalled ome, ome-keeper and omed into ${bin}`);
console.log("Put them on PATH (add this to your shell profile):\n");
console.log(`  export PATH="${bin}:$PATH"\n`);
console.log("Shell completions: ome completions zsh > ~/.zfunc/_ome (or bash, fish)");
console.log("Then: ome init");
