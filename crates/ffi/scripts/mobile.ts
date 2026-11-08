// Builds and tests the mobile bindings of ome-ffi. Every output goes to crates/ffi/dist (gitignored).
//
//   node scripts/mobile.ts swift     host test: macOS static library + generated Swift, `swift test`
//   node scripts/mobile.ts kotlin    host test: host library + generated Kotlin on the JVM, `gradle test`
//   node scripts/mobile.ts apple     dist/apple: OmeFfi.xcframework (iOS, iOS Simulator, macOS) and OmeFfi.swift
//   node scripts/mobile.ts android   dist/android: ome-ffi.aar (arm64-v8a, x86_64) built with cargo-ndk
//
// Libraries use the `mobile` profile (size-optimized release). The iOS and Android Rust targets are added on demand
// (`rustup target add`), not in rust-toolchain.toml, so other jobs never download them. Android needs
// ANDROID_NDK_HOME and cargo-ndk; the Kotlin steps need a JDK 17+ and Gradle.

import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const crate = join(dirname(fileURLToPath(import.meta.url)), "..");
const workspace = join(crate, "..");
const manifest = join(workspace, "Cargo.toml");
const target = join(workspace, "target");
const dist = join(crate, "dist");
const PROFILE = "mobile";
const APPLE = ["aarch64-apple-ios", "aarch64-apple-ios-sim", "aarch64-apple-darwin"] as const;
const ANDROID = { "arm64-v8a": "aarch64-linux-android", x86_64: "x86_64-linux-android" } as const;
/** Oldest OS versions the libraries run on; C code in dependencies is compiled for these too. */
const DEPLOYMENT = { MACOSX_DEPLOYMENT_TARGET: "14.0", IPHONEOS_DEPLOYMENT_TARGET: "16.0" };

function run(command: string, args: string[], env: NodeJS.ProcessEnv = {}): void {
  execFileSync(command, args, { cwd: crate, stdio: "inherit", env: { ...process.env, ...env } });
}

function cargo(args: string[]): void {
  run(
    "cargo",
    [...args, "--manifest-path", manifest, "-p", "ome-ffi", "--profile", PROFILE],
    DEPLOYMENT,
  );
}

/** The host library, built for tests and to generate the bindings from. */
function host(): string {
  cargo(["build"]);
  return join(target, PROFILE);
}

/** Writes the Swift or Kotlin bindings of `library` into `out`. */
function generate(library: string, language: "swift" | "kotlin", out: string): void {
  rmSync(out, { recursive: true, force: true });
  run("cargo", [
    "run",
    "--quiet",
    "--manifest-path",
    manifest,
    "-p",
    "ome-ffi",
    "--features",
    "bindgen",
    "--bin",
    "uniffi-bindgen",
    "--",
    "generate",
    "--library",
    library,
    "--language",
    language,
    "--no-format",
    "--out-dir",
    out,
  ]);
}

/** A SwiftPM layout of the generated Swift: the C module and the Swift module. */
function swiftSources(generated: string, sources: string): void {
  const include = join(sources, "OmeFfiFFI", "include");
  mkdirSync(include, { recursive: true });
  renameSync(join(generated, "OmeFfiFFI.h"), join(include, "OmeFfiFFI.h"));
  renameSync(join(generated, "OmeFfiFFI.modulemap"), join(include, "module.modulemap"));
  // SwiftPM needs a source file in a C target.
  writeFileSync(join(sources, "OmeFfiFFI", "shim.c"), "// The header is the module.\n");
  mkdirSync(join(sources, "OmeFfi"), { recursive: true });
  renameSync(join(generated, "OmeFfi.swift"), join(sources, "OmeFfi", "OmeFfi.swift"));
}

function swift(): void {
  const lib = host();
  const out = join(dist, "swift");
  rmSync(out, { recursive: true, force: true });
  cpSync(join(crate, "bindings", "swift"), out, { recursive: true });
  const generated = join(dist, "generated-swift");
  generate(join(lib, "libome_ffi.dylib"), "swift", generated);
  swiftSources(generated, join(out, "Sources"));
  rmSync(generated, { recursive: true, force: true });
  run("swift", ["test", "--package-path", out], {
    OME_FFI_STATIC_LIB: join(lib, "libome_ffi.a"),
  });
}

/** The Kotlin project with the generated bindings; returns its directory. */
function kotlinProject(library: string): string {
  const out = join(dist, "kotlin");
  rmSync(out, { recursive: true, force: true });
  cpSync(join(crate, "bindings", "kotlin"), out, { recursive: true });
  generate(library, "kotlin", join(out, "src", "main", "kotlin"));
  return out;
}

/** The host's dynamic library in `dir`. */
function hostLibrary(dir: string): string {
  const library = ["libome_ffi.dylib", "libome_ffi.so"]
    .map((name) => join(dir, name))
    .find((path) => exists(path));
  if (!library) throw new Error("no host library was built");
  return library;
}

function kotlin(): void {
  const lib = host();
  const project = kotlinProject(hostLibrary(lib));
  run("gradle", ["--no-daemon", "--quiet", "-p", project, "test"], { OME_FFI_LIB_DIR: lib });
}

/** Installs the Rust targets a build needs (a no-op when they are there), so other jobs never download them. */
function targets(triples: readonly string[]): void {
  run("rustup", ["target", "add", ...triples]);
}

function apple(): void {
  targets(APPLE);
  for (const triple of APPLE) cargo(["build", "--target", triple]);
  const out = join(dist, "apple");
  rmSync(out, { recursive: true, force: true });
  const generated = join(dist, "generated-swift");
  generate(join(target, "aarch64-apple-darwin", PROFILE, "libome_ffi.a"), "swift", generated);
  const headers = join(dist, "apple-headers");
  rmSync(headers, { recursive: true, force: true });
  mkdirSync(headers, { recursive: true });
  renameSync(join(generated, "OmeFfiFFI.h"), join(headers, "OmeFfiFFI.h"));
  renameSync(join(generated, "OmeFfiFFI.modulemap"), join(headers, "module.modulemap"));
  mkdirSync(out, { recursive: true });
  renameSync(join(generated, "OmeFfi.swift"), join(out, "OmeFfi.swift"));
  run("xcodebuild", [
    "-create-xcframework",
    ...APPLE.flatMap((triple) => [
      "-library",
      join(target, triple, PROFILE, "libome_ffi.a"),
      "-headers",
      headers,
    ]),
    "-output",
    join(out, "OmeFfi.xcframework"),
  ]);
  rmSync(generated, { recursive: true, force: true });
  rmSync(headers, { recursive: true, force: true });
  for (const triple of APPLE) report(triple, join(target, triple, PROFILE, "libome_ffi.a"));
}

function android(): void {
  targets(Object.values(ANDROID));
  run("cargo", [
    "ndk",
    ...Object.keys(ANDROID).flatMap((abi) => ["-t", abi]),
    "build",
    "--manifest-path",
    manifest,
    "-p",
    "ome-ffi",
    "--profile",
    PROFILE,
  ]);
  const libraries = Object.entries(ANDROID).map(([abi, triple]) => ({
    abi,
    path: join(target, triple, PROFILE, "libome_ffi.so"),
  }));
  // Stripped Android libraries keep no UniFFI metadata; the host library has the same.
  const project = kotlinProject(hostLibrary(host()));
  run("gradle", ["--no-daemon", "--quiet", "-p", project, "jar"]);
  const aar = join(dist, "android", "aar");
  rmSync(join(dist, "android"), { recursive: true, force: true });
  mkdirSync(aar, { recursive: true });
  writeFileSync(
    join(aar, "AndroidManifest.xml"),
    '<?xml version="1.0" encoding="utf-8"?>\n' +
      '<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="labs.ome.ffi">\n' +
      '  <uses-permission android:name="android.permission.INTERNET" />\n' +
      "</manifest>\n",
  );
  writeFileSync(join(aar, "R.txt"), "");
  cpSync(join(project, "build", "libs", "ome-ffi.jar"), join(aar, "classes.jar"));
  for (const { abi, path } of libraries) {
    mkdirSync(join(aar, "jni", abi), { recursive: true });
    cpSync(path, join(aar, "jni", abi, "libome_ffi.so"));
    report(abi, path);
  }
  execFileSync("zip", ["-q", "-r", join(dist, "android", "ome-ffi.aar"), "."], { cwd: aar });
  rmSync(aar, { recursive: true, force: true });
}

function exists(path: string): boolean {
  try {
    statSync(path);
    return true;
  } catch {
    return false;
  }
}

function report(name: string, path: string): void {
  const mib = statSync(path).size / (1024 * 1024);
  console.log(`${name.padEnd(24)} ${mib.toFixed(1)} MiB  ${path.slice(workspace.length + 1)}`);
}

const commands = { swift, kotlin, apple, android } as const;
const command = process.argv[2] as keyof typeof commands | undefined;
if (!command || !(command in commands)) {
  console.error(`usage: node scripts/mobile.ts ${Object.keys(commands).join("|")}`);
  process.exit(2);
}
commands[command]();
