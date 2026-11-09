#!/usr/bin/env node
// Packs every published workspace package into <out>, as npm will receive it: `pnpm pack` turns `workspace:*` into
// exact versions and applies `publishConfig`. Each package gets the repository's LICENSE, and @ome-labs/ome gets its
// platform packages as optional dependencies (pinned to the release version; they are not workspace packages).
//
//   node scripts/packaging/npm.ts <out>
//
// Build first (`pnpm build`).
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { PACKAGES, PLATFORMS, args, root, version } from "./targets.ts";

const [outArg] = args("npm.ts", ["out"]);
const out = resolve(outArg);
mkdirSync(out, { recursive: true });
const release = version();

for (const { name, dir } of PACKAGES) {
  const path = join(root, dir);
  if (!existsSync(join(path, "LICENSE"))) {
    copyFileSync(join(root, "LICENSE"), join(path, "LICENSE"));
  }
  const manifestPath = join(path, "package.json");
  const original = readFileSync(manifestPath, "utf8");
  if (name === "@ome-labs/ome") {
    const manifest = JSON.parse(original) as Record<string, unknown>;
    manifest.optionalDependencies = Object.fromEntries(
      PLATFORMS.map((p) => [`@ome-labs/ome-${p.name}`, release]),
    );
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  }
  try {
    execFileSync("pnpm", ["pack", "--pack-destination", out], { cwd: path, stdio: "inherit" });
  } finally {
    writeFileSync(manifestPath, original);
  }
}
