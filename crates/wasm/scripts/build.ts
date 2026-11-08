// Builds @ome-labs/wasm: compiles ome-wasm for wasm32-unknown-unknown (profile `wasm`) without and with the `testing`
// feature, and runs the wasm-bindgen CLI for Node (CommonJS) and the browser (ES module, `await init()` first).
//
//   node scripts/build.ts              build dist/{main,testing}/{node,web}
//   node scripts/build.ts --generate   regenerate src/types.d.ts from the Rust types, formatted

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const crate = join(dirname(fileURLToPath(import.meta.url)), "..");
const workspace = join(crate, "..");
const manifest = join(workspace, "Cargo.toml");
const dist = join(crate, "dist");

const run = (command: string, args: string[]): string =>
  execFileSync(command, args, {
    cwd: crate,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
  });

/** The wasm-bindgen version Cargo.lock pins; the CLI must match it exactly. */
function lockedBindgen(): string {
  const lock = readFileSync(join(workspace, "Cargo.lock"), "utf8");
  const version = /name = "wasm-bindgen"\nversion = "([^"]+)"/u.exec(lock)?.[1];
  if (!version) throw new Error("wasm-bindgen is not in crates/Cargo.lock");
  return version;
}

function checkCli(): void {
  const wanted = lockedBindgen();
  let installed = "";
  try {
    installed = run("wasm-bindgen", ["--version"]).trim().split(" ")[1] ?? "";
  } catch {
    // Not installed: reported below.
  }
  if (installed !== wanted) {
    throw new Error(
      `wasm-bindgen CLI ${installed || "not found"}; install ${wanted}: ` +
        `cargo install wasm-bindgen-cli --version ${wanted} --locked`,
    );
  }
}

function generate(): void {
  const declarations = run("cargo", [
    "run",
    "--quiet",
    "--manifest-path",
    manifest,
    "-p",
    "ome-wasm",
    "--example",
    "typescript",
  ]);
  const path = join(crate, "src", "types.d.ts");
  writeFileSync(path, declarations);
  run("oxfmt", [path]);
}

function build(variant: "main" | "testing"): void {
  const features = variant === "testing" ? ["--features", "testing"] : [];
  run("cargo", [
    "build",
    "--manifest-path",
    manifest,
    "-p",
    "ome-wasm",
    "--target",
    "wasm32-unknown-unknown",
    "--profile",
    "wasm",
    ...features,
  ]);
  const module = join(workspace, "target", "wasm32-unknown-unknown", "wasm", "ome_wasm.wasm");
  for (const target of ["nodejs", "web"] as const) {
    const out = join(dist, variant, target === "nodejs" ? "node" : "web");
    run("wasm-bindgen", [module, "--target", target, "--out-dir", out, "--out-name", "ome_wasm"]);
  }
  // The package is ESM; wasm-bindgen's Node output is CommonJS.
  writeFileSync(join(dist, variant, "node", "package.json"), '{ "type": "commonjs" }\n');
}

if (process.argv.includes("--generate")) {
  generate();
} else {
  checkCli();
  rmSync(dist, { recursive: true, force: true });
  mkdirSync(dist, { recursive: true });
  build("main");
  build("testing");
}
