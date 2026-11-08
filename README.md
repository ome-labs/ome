# Ome

A private k-of-n multisig on [Aztec](https://aztec.network) with threshold viewing: `k` members to spend, `t` members
to read, and nobody ever holds the vault's viewing key.

This repository is being rebuilt from scratch, component by component. It currently contains the workspace scaffold
only.

## Layout

```text
contracts/   Nargo workspace (Aztec.nr 6.0.0)
crates/      Cargo workspace (Rust)
packages/    TypeScript packages (pnpm)
scripts/     repository scripts
```

## Toolchain

| Concern     | Tool                                                                                                                |
| ----------- | ------------------------------------------------------------------------------------------------------------------- |
| Contracts   | Aztec 6.0.0 (`aztec compile`, `aztec test`, `aztec-nargo fmt / check`)                                              |
| Rust        | 1.99.1 (`rust-toolchain.toml`), `rustfmt`, `clippy -D warnings`, `cargo-deny`                                       |
| WebAssembly | `wasm32-unknown-unknown` (`rust-toolchain.toml`), `wasm-bindgen` CLI 0.2.108                                        |
| Mobile      | UniFFI 0.32.2; iOS and Android targets (added by the build script), Xcode, JDK 21, Gradle, Android NDK, `cargo-ndk` |
| TypeScript  | Node 24, pnpm 12, Turborepo, TypeScript 7, oxfmt, oxlint (klarity presets)                                          |
| Hooks       | lefthook, commitlint (Conventional Commits)                                                                         |

Install Aztec with `aztec-up install 6.0.0`, `cargo-deny` with `cargo install cargo-deny`, and the `wasm-bindgen` CLI
with `cargo install wasm-bindgen-cli --version 0.2.108 --locked` (it must match the `wasm-bindgen` crate in
`crates/Cargo.lock`).

The mobile bindings (`crates/ffi`) also need Xcode (iOS), and for Android a JDK 21, Gradle, the Android
command-line tools with an NDK (`sdkmanager "platform-tools" "platforms;android-37.2" "ndk;30.0.16248370"`), and
`cargo install cargo-ndk`; set `JAVA_HOME`, `ANDROID_HOME` and `ANDROID_NDK_HOME`.

## Commands

```sh
pnpm install        # also installs git hooks
pnpm check          # format check, lint (oxlint, clippy, nargo check), typecheck, build, cargo-deny
pnpm test           # aztec test, cargo test, vitest
pnpm format         # oxfmt, rustfmt, nargo fmt
```

Contracts:

```sh
pnpm --filter @ome-labs/contracts test                  # unit, integration (TXE) and end-to-end tests
pnpm --filter @ome-labs/contracts check:failures        # every assert message is expected by a test
pnpm --filter @ome-labs/contracts profile               # gate counts per function
pnpm --filter @ome-labs/contracts profile:flamegraphs   # target/flamegraphs/<artifact>/<function>.svg
```

Daemon (`@ome-labs/daemon`):

```sh
pnpm --filter @ome-labs/daemon test       # unit tests (no network)
pnpm --filter @ome-labs/daemon test:e2e   # three or four daemons against `aztec start --local-network`
node packages/daemon/src/omed.ts --profile <name>   # run omed
```

Wallet (`@ome-labs/wallet`):

```sh
pnpm --filter @ome-labs/wallet test       # unit tests (no network)
pnpm --filter @ome-labs/wallet test:e2e   # against `aztec start --local-network`, with real ome-keeper processes
```

WebAssembly (`@ome-labs/wasm`):

```sh
pnpm --filter @ome-labs/wasm build      # dist/{main,testing}/{node,web}
pnpm --filter @ome-labs/wasm test       # Vitest against the built package
pnpm --filter @ome-labs/wasm generate   # src/types.d.ts from the Rust boundary types
pnpm --filter @ome-labs/wasm bench      # timings of combine, outer hash and unseal
```

Mobile bindings (`@ome-labs/ffi`, `crates/ffi`):

```sh
pnpm --filter @ome-labs/ffi mobile:swift     # Swift bindings on the macOS host: swift test
pnpm --filter @ome-labs/ffi mobile:kotlin    # Kotlin bindings on the JVM: gradle test
pnpm --filter @ome-labs/ffi mobile:apple     # dist/apple/OmeFfi.xcframework and OmeFfi.swift
pnpm --filter @ome-labs/ffi mobile:android   # dist/android/ome-ffi.aar
```

## License

Copyright 2026 Vedant Chainani. Licensed under the [Apache License, Version 2.0](LICENSE).
