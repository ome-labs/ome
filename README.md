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

| Concern    | Tool                                                                          |
| ---------- | ----------------------------------------------------------------------------- |
| Contracts  | Aztec 6.0.0 (`aztec compile`, `aztec test`, `aztec-nargo fmt / check`)        |
| Rust       | 1.99.1 (`rust-toolchain.toml`), `rustfmt`, `clippy -D warnings`, `cargo-deny` |
| TypeScript | Node 24, pnpm 12, Turborepo, TypeScript 7, oxfmt, oxlint (klarity presets)    |
| Hooks      | lefthook, commitlint (Conventional Commits)                                   |

Install Aztec with `aztec-up install 6.0.0` and `cargo-deny` with `cargo install cargo-deny`.

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

## License

Copyright 2026 Vedant Chainani. Licensed under the [Apache License, Version 2.0](LICENSE).
