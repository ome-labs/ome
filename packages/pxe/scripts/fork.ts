// Builds @ome-labs/pxe: Aztec's published @aztec-labs/pxe `dest/` with Ome's patch applied
// (`patches/@aztec-labs__pxe@6.0.0.patch`, the single source of the change). Fails when the patch no longer applies or
// when this package's dependencies or exports drift from upstream's, which is what an Aztec upgrade looks like.
//
//   node scripts/fork.ts
import { execFileSync } from "node:child_process";
import { cp, readFile, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const patch = join(root, "..", "..", "patches", "@aztec-labs__pxe@6.0.0.patch");
// `@aztec-labs/pxe/server` resolves to `<package>/dest/entrypoints/server/index.js`.
const upstream = fileURLToPath(new URL("../../../", import.meta.resolve("@aztec-labs/pxe/server")));

type Manifest = {
  version: string;
  dependencies: Record<string, string>;
  exports: Record<string, string>;
};
const read = async (path: string) => JSON.parse(await readFile(path, "utf8")) as Manifest;
const theirs = await read(join(upstream, "package.json"));
const ours = await read(join(root, "package.json"));

const { "./bin": _bin, ...exports } = theirs.exports;
const drift = [
  JSON.stringify(ours.dependencies) !== JSON.stringify(theirs.dependencies) && "dependencies",
  JSON.stringify(ours.exports) !== JSON.stringify(exports) && "exports",
  !ours.version.startsWith(`${theirs.version}-ome.`) && "version",
].filter(Boolean);
if (drift.length > 0) {
  console.error(
    `packages/pxe/package.json differs from @aztec-labs/pxe@${theirs.version} in: ${drift.join(", ")}`,
  );
  process.exit(1);
}

const dest = join(root, "dest");
await rm(dest, { recursive: true, force: true });
await cp(join(upstream, "dest"), dest, {
  recursive: true,
  // Source maps point into upstream's `src/`, which the fork does not ship.
  filter: (path) => !/\.test\.|\.map$/u.test(path),
});
execFileSync("patch", ["-p1", "--forward", "--quiet", "-d", root, "-i", patch], {
  stdio: "inherit",
});
console.log(`@ome-labs/pxe ${ours.version}: patched dest/ from @aztec-labs/pxe ${theirs.version}`);
