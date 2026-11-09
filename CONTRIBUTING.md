# Contributing

How code is written, tested and committed in this repository. Security issues go through [SECURITY.md](SECURITY.md),
not public issues.

## Setup

| Concern     | Tool                                                                                                               |
| ----------- | ------------------------------------------------------------------------------------------------------------------ |
| Contracts   | Aztec 6.0.0 (`aztec compile`, `aztec test`, `aztec-nargo fmt / check`); install with `aztec-up install 6.0.0`      |
| Rust        | 1.99.1 (`rust-toolchain.toml`), `rustfmt`, `clippy -D warnings`, `cargo-deny` (`cargo install cargo-deny`)         |
| WebAssembly | `wasm32-unknown-unknown`, `wasm-bindgen` CLI 0.2.108 (`cargo install wasm-bindgen-cli --version 0.2.108 --locked`) |
| Mobile      | UniFFI 0.32.2; Xcode for iOS; JDK 21, Gradle, the Android NDK and `cargo-ndk` for Android                          |
| TypeScript  | Node 24 (`.node-version`), pnpm 12, Turborepo, TypeScript 7, oxfmt, oxlint (klarity presets)                       |
| Hooks       | lefthook, commitlint (Conventional Commits)                                                                        |

The `wasm-bindgen` CLI must match the `wasm-bindgen` crate in `crates/Cargo.lock`. For Android, install
`sdkmanager "platform-tools" "platforms;android-37.2" "ndk;30.0.16248370"` and set `JAVA_HOME`, `ANDROID_HOME` and
`ANDROID_NDK_HOME`.

```sh
pnpm install        # also installs the git hooks
pnpm check          # format check, lint (oxlint, clippy, nargo check), typecheck, build, cargo-deny
pnpm test           # aztec test, cargo test, vitest
pnpm format         # oxfmt, rustfmt, nargo fmt
pnpm cli:install    # build ome, ome-keeper and an omed wrapper for this checkout into ~/.ome/bin
```

Narrower lanes:

```sh
pnpm --filter @ome-labs/contracts test                  # contract unit, integration (TXE) and end-to-end tests
pnpm --filter @ome-labs/contracts check:failures        # every assert message is expected by a test
pnpm --filter @ome-labs/contracts profile               # gate counts per function
cargo test --manifest-path crates/Cargo.toml -p <crate> # one crate, e.g. ome-keeper or ome-cli
pnpm --filter @ome-labs/wasm build|test                 # the WebAssembly package
pnpm --filter @ome-labs/wallet test                     # wallet unit tests (no network)
pnpm --filter @ome-labs/daemon test                     # daemon unit tests (no network)
pnpm --filter @ome-labs/ffi mobile:swift|mobile:kotlin  # the phone bindings
```

Slow suites, outside `pnpm test` (Docker; the first three need `aztec start --local-network`, which `--start` starts
for you):

```sh
pnpm test:e2e         # wallet and daemons end to end: founding, paying, rotating, a phone, a keeper on a server
pnpm test:removal     # the member removal drill, 2-of-3 and 3-of-4
pnpm test:cli         # three members through `ome` only
pnpm test:network     # a relay and three keepers on separate Docker networks
pnpm bench:proving    # timings with real client-side proofs
pnpm drill:testnet    # found, fund, pay and rotate on Aztec Testnet with real proofs
```

CI runs the fast checks on every push to `main` (`.github/workflows/ci.yml`); the slow suites and the Testnet drill
run by hand (`.github/workflows/extended.yml`). Releases are described in
[architecture/release.md](architecture/release.md).

## Workflow

- Inspect the branch, working tree and recent commits before changing anything; keep unrelated work intact.
- Commit after each coherent change with a [Conventional Commit](https://www.conventionalcommits.org/). Scopes are the
  component's short name: `contracts`, `crypto`, `wire`, `protocol`, `signer`, `net`, `keeper`, `relay`, `api`,
  `client`, `cli`, `wasm`, `ffi`, `wallet`, `pxe`, `daemon`, `repo`. The release pull request and `CHANGELOG.md` are
  written from these messages: `feat` and `fix` appear in the changelog, and a `!` or `BREAKING CHANGE:` footer marks a
  breaking change. Commit messages are checked by commitlint (body lines up to 100 characters).
- Run the narrowest relevant check first, then `pnpm check` and `pnpm test`. Report skipped lanes and pre-existing
  failures rather than hiding them.
- Update the owning page in `architecture/` in the same change when a contract, wire format, key flow or trust
  boundary changes.
- Open a pull request against `main`; CI must pass. Protocol `v0` is frozen from the first release: wire formats, the
  contract and sealed files change only with a migration.

## Code

- **One responsibility per file**, grouped into domain-named folders (no `utils`, `helpers` or `common`). Keep files
  under roughly 400–500 lines and split them at real boundaries when they grow.
- **Spacing is part of the code**: blank lines separate the logical steps of a function (inputs, checks, the main
  operation, side effects, the result).
- **Keep behaviour visible**: inline one-off logic; extract a function only for real reuse, an important domain
  concept, or substantial complexity behind a precise name.
- **Comment on reasoning**, not on what the code already says: why an approach was chosen, invariants, ordering
  constraints, external limitations.
- **Secrets**: all threshold cryptography lives in `ome-crypto`; constant-time comparisons, `zeroize` on secret types,
  no secret in logs or `Debug` output.

## Testing

The aim is the smallest set of tests that would catch a real regression: every test must protect a behaviour someone
relies on.

### What deserves a test

Write a test when it protects one of these:

1. **A public behaviour** of the crate: what a caller observes through its API.
2. **A security property**: a forged partial is rejected, a tampered frame fails to decode, a secret is never
   accepted under the wrong context, a replayed message is refused.
3. **A compatibility contract**: byte-for-byte agreement with Aztec 6.0.0, with the Noir contract, or between the
   Rust, WASM and TypeScript encoders.
4. **An error path a caller handles**: each error variant a caller matches on is produced by at least one test.
5. **A bug that happened**: a regression test named after the behaviour, not the ticket.

Do **not** write a test for:

- what the type system already guarantees (field access, constructors that only move values, `Clone`/`Debug` derives);
- private helpers already exercised through the public API;
- third-party behaviour (that arkworks multiplies points, that serde serialises a struct);
- the same behaviour at several levels: if an integration test covers it, skip the unit test, and vice versa;
- trivial getters, constants restated as literals, or "it compiles" checks.

### Where tests live (Rust)

| Kind                | Location                                                  | Use for                                                                                                     |
| ------------------- | --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Unit                | `#[cfg(test)] mod tests` at the bottom of the file        | internal logic that is hard to reach through the public API (a parser state, a math helper with edge cases) |
| Integration         | `crates/<name>/tests/<feature>.rs`                        | the public API, one file per feature (`dkg.rs`, `threshold_ecdh.rs`, `codec.rs`)                            |
| Shared test support | `crates/<name>/tests/support/mod.rs`                      | fixtures and builders shared by integration tests; named by domain, never `utils`                           |
| Golden vectors      | `crates/<name>/tests/vectors/*.json`                      | compatibility with Aztec or other encoders; generated by a script, never edited by hand                     |
| Property tests      | inside the feature's integration file, behind `proptest!` | algebraic laws (any t of n shares combine to the same point; decode ∘ encode = id)                          |
| Fuzz targets        | `crates/<name>/fuzz/` (cargo-fuzz, own workspace)         | decoders and parsers that take untrusted bytes                                                              |

Binary crates (`ome-keeper`, `ome-relay`) keep `main.rs` thin: argument parsing and startup only. Everything worth
testing lives in the library and is tested there.

### How to write them

- **Test features, not code.** Name tests after the behaviour: `rejects_a_partial_with_a_forged_proof`, not
  `test_verify_2`. A refactor that keeps behaviour must not break tests.
- **One `check` helper per feature** when several tests share a shape, so API changes touch one place.
- **Data-driven where cases repeat.** A table of `(input, expected)` beats ten near-identical functions.
- **Deterministic randomness.** Seed RNGs (`ChaCha20Rng::seed_from_u64`) in tests; property tests use proptest's
  own seeds and persist failures.
- **No sleeps.** Wait on explicit signals (a channel, a completed future). Tests that need wall-clock time inject a
  clock.
- **No mocks for our own code.** Test through real components; inject boundaries that do I/O (clock, randomness,
  storage location, transport). The in-memory transport is a real implementation, not a mock.
- **Assert outcomes precisely.** Match the exact error variant (`assert!(matches!(err, Error::InvalidProof(3)))`), not
  just `is_err()`.
- **Errors over panics.** Library code returns `Result`; reserve `#[should_panic]` for genuine invariants.
- **Plain asserts.** `assert_eq!` and `assert!(matches!(..))`; no assertion libraries.
- **Spacing and size**: arrange, act and assert separated by blank lines; test files under
  ~400–500 lines, split by feature when they grow.

### Speed tiers

| Tier    | Runs in                       | Contents                                                                 |
| ------- | ----------------------------- | ------------------------------------------------------------------------ |
| Default | `cargo test`, every commit    | unit, integration, golden vectors, property tests at a modest case count |
| Slow    | `cargo test -- --ignored`, CI | many-member ceremonies, long property runs                               |
| System  | own scripts, CI on main       | multi-process keepers, Docker network, local Aztec network               |

Mark slow tests `#[ignore = "slow: <reason>"]` so the reason shows in the output.

### Golden vectors

- Generated by `tools/vectors` (`@ome-labs/vectors`) from Aztec 6.0.0's own packages, never from Ome code.
- Checked in as JSON whose header names the generator (`source`) and the Aztec version.
- Each crate that claims compatibility has one integration test file that loads its vectors and checks every entry.
- Regenerated on every Aztec upgrade; a diff in vectors is a protocol change and needs review.

### TypeScript packages

- Vitest; unit tests beside the module (`foo.test.ts`), end-to-end suites in `test/e2e/`.
- Same principles: test behaviour through the package's public exports; no mocks of our own modules; real
  `@ome-labs/wasm` in tests.
- Anything that needs a running Aztec network is a separate suite (`test:e2e`), not part of the default `test`.

### Checks that are not tests

These run in CI and replace tests that would only restate them: `cargo fmt --check`, `clippy -D warnings`,
`cargo deny check`, `oxfmt`, `oxlint`, `tsc`, the golden-vector freshness check, and the contract's
failure-message check.
