// The native programs of this platform, from the @ome-labs/ome-<os>-<arch> package npm installed beside this one
// (optionalDependencies, one per platform; npm skips the others by their `os` and `cpu`).
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);

/** Platforms with a published binary package. */
const PLATFORMS = ["darwin-arm64", "linux-x64", "linux-arm64"];

/**
 * Path of `name` for this platform; exits with a clear message when there is none.
 * @param {"ome" | "ome-keeper"} name
 * @returns {string}
 */
export function native(name) {
  const platform = `${process.platform}-${process.arch}`;
  try {
    return join(dirname(require.resolve(`@ome-labs/ome-${platform}/package.json`)), "bin", name);
  } catch {
    const supported = PLATFORMS.includes(platform)
      ? `@ome-labs/ome-${platform} is missing: reinstall with optional dependencies enabled (npm i -g @ome-labs/ome).`
      : `Ome has no build for ${platform} yet; it runs on ${PLATFORMS.join(", ")}.`;
    process.stderr.write(`ome: ${supported}\n`);
    process.exit(1);
  }
}

/** Path of the daemon's entry point, from @ome-labs/daemon. */
export function daemon() {
  return join(dirname(require.resolve("@ome-labs/daemon/package.json")), "dist", "omed.js");
}
