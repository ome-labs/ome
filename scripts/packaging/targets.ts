// What a release builds: the platforms with native binaries, and the npm packages, in publishing order.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

export interface Platform {
  /** `<os>-<arch>` as Node names them: the npm package suffix and the archive name. */
  name: string;
  /** The Rust target the binaries are built for. */
  rust: string;
  os: string;
  cpu: string;
}

export const PLATFORMS: readonly Platform[] = [
  { name: "darwin-arm64", rust: "aarch64-apple-darwin", os: "darwin", cpu: "arm64" },
  { name: "linux-x64", rust: "x86_64-unknown-linux-gnu", os: "linux", cpu: "x64" },
  { name: "linux-arm64", rust: "aarch64-unknown-linux-gnu", os: "linux", cpu: "arm64" },
];

export function platform(name: string): Platform {
  const found = PLATFORMS.find((p) => p.name === name);
  if (!found) {
    throw new Error(`unknown platform ${name}; one of ${PLATFORMS.map((p) => p.name).join(", ")}`);
  }
  return found;
}

/** The workspace packages published to npm, dependencies first; directories relative to the root. */
export const PACKAGES = [
  { name: "@ome-labs/api", dir: "packages/api" },
  { name: "@ome-labs/wasm", dir: "crates/wasm" },
  { name: "@ome-labs/contracts", dir: "contracts" },
  { name: "@ome-labs/pxe", dir: "packages/pxe" },
  { name: "@ome-labs/wallet", dir: "packages/wallet" },
  { name: "@ome-labs/daemon", dir: "packages/daemon" },
  { name: "@ome-labs/ome", dir: "packages/ome" },
] as const;

/** The release version: every package but the PXE fork carries it. */
export function version(): string {
  const manifest = JSON.parse(readFileSync(join(root, "packages/ome/package.json"), "utf8")) as {
    version: string;
  };
  return manifest.version;
}

/** The npm tarball name `pnpm pack` and `npm pack` give a package. */
export function tarball(name: string, packageVersion: string): string {
  return `${name.replace("@", "").replace("/", "-")}-${packageVersion}.tgz`;
}

/** The positional arguments `names` describes, or a usage message and exit. */
export function args<const T extends readonly string[]>(
  script: string,
  names: T,
): { [K in keyof T]: string } {
  const given = process.argv.slice(2);
  if (given.length !== names.length) {
    process.stderr.write(`usage: ${script} ${names.map((n) => `<${n}>`).join(" ")}\n`);
    process.exit(2);
  }
  return given as { [K in keyof T]: string };
}
