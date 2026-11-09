# Ome

A private k-of-n multisig on [Aztec](https://aztec.network) with threshold viewing: `k` members to spend, `t` members
to read, and nobody ever holds the vault's viewing key.

## Status

The core is complete and tested as a system: the vault contract, the Rust core (cryptography, wire, protocol, signer,
networking, keeper, relay, API), the WebAssembly and mobile bindings, the PXE host, the member's daemon (`omed`) and
their command line (`ome`).

**What works**, end to end on Aztec 6.0.0's local network, and on Testnet with real proofs:

- founding a k-of-n vault through a lobby and a DKG, with every member deriving the same address;
- paying with `k` approvals, rejecting, cancelling; a removed member's open approval going stale;
- reading the vault's notes through threshold ECDH, the viewing key never in one place;
- adding and removing members with a reshare, the vault keeping its address, and the contract refusing a committee that
  does not hold the vault's viewing key; a removed member's keeper, even
  restarted from before the removal, cannot read anything sent afterwards (`pnpm test:removal`);
- keepers on separate networks reaching each other only through a relay, and catching up through its mailboxes
  (`pnpm test:network`);
- a member's signing key on a phone (`crates/ffi`, Swift and Kotlin), enrolled with a signer swap;
- a member's keeper on their own server, reached over TLS pinned by fingerprint;
- received transfers in a vault's activity, and the counterparties a removed member could still notice.

**Measured:** see `pnpm bench:proving` and the architecture pages; the numbers from the latest run are in
[architecture/README.md](architecture/README.md#measurements).

**Lost keys and shares:** a vault needs `k` signing keys to spend and `t_v` keepers to read, and cannot spend what it
cannot read. A lost key or keeper is replaced by a signer swap or a membership change while enough remain;
`ome vault health` shows the margin. Shares are never backed up. See [architecture/recovery.md](architecture/recovery.md).

**Not built yet:** the desktop and mobile apps (the command line is the way in today); a recovery keeper (a viewer
without a signing key); push notifications; P-256 signer keys held by the Secure Enclave or StrongBox; an external
audit.

## Layout

```text
contracts/   Nargo workspace (Aztec.nr 6.0.0): the vault contract
crates/      Cargo workspace (Rust): crypto, wire, protocol, signer, net, keeper, relay, api, client, cli, wasm, ffi
packages/    TypeScript: the generated API, the wallet (PXE host) and the daemon (omed)
tools/       golden vectors and the multi-network suite
scripts/     repository scripts
architecture/ design pages: what is built, decided and measured
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

## Using Ome

```sh
pnpm cli:install                  # ome, ome-keeper and omed into ~/.ome/bin; prints the PATH line
ome init                          # your profile: name, network, passphrase
ome vault create                  # found a vault with co-founders: prints the invite
ome vault join <invite>           # each co-founder
ome vault receive                 # the vault's address, and the aztec-wallet line to fund it
ome propose transfer 250 USDC bob # a payment; members review and approve it
ome approve 4f2a                  # the review on your device, then your signature
ome execute 4f2a                  # prove and send once enough approved
ome watch                         # proposals and activity, live
```

`ome --help` lists every command; [architecture/cli.md](architecture/cli.md) describes them, and how `ome` works
alongside `aztec-wallet`.

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

Slow suites (each needs Docker; the first three `aztec start --local-network`, which `--start` starts for you):

```sh
pnpm test:e2e         # wallet and daemons end to end: founding, paying, rotating, a phone, a keeper on a server
pnpm test:removal     # the member removal drill, 2-of-3 and 3-of-4
pnpm test:cli         # three members through `ome` only: founding, paying, removing a member
pnpm test:network     # a relay and three keepers on separate Docker networks
pnpm bench:proving    # timings with real client-side proofs
pnpm drill:testnet    # found, fund, pay and rotate on Aztec Testnet with real proofs
```

CI runs the fast checks on every push to `main` (`.github/workflows/ci.yml`); the slow suites, the mobile bindings
and, on request, the Testnet drill run by hand (`.github/workflows/extended.yml`).

Command line (`ome-cli`, `crates/cli`):

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-cli   # parsing, references, renderers, a stand-in daemon
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
