# Releases

How Ome is versioned, built, published and installed, and how to check that what you run came from this repository.

## Versions

- One version for everything a release ships: the npm packages, the archives, the images, `ome --version`,
  `ome-keeper --version`, `ome-relay --version` and the daemon's `daemon.info`. The command line, daemon and keeper
  of one install must match; `ome doctor` reports a mismatch.
- release-please (`release-please-config.json`) derives the next version from the Conventional Commits on `main`.
  Before 1.0 a breaking change bumps the minor version. The version lives in `.release-please-manifest.json`, every
  published `package.json`, and three annotated constants (`// x-release-please-version`): `ome_api::RELEASE_VERSION`,
  the relay's `VERSION` and the daemon's `DAEMON_VERSION`. The Cargo workspace stays at `0.0.0`: no crate is
  published, and a moving Cargo version would rewrite `Cargo.lock` on every release.
- The first release is pinned to `0.1.0` with `release-as` in `release-please-config.json` (release-please starts a
  new project at `1.0.0`). Remove that line once `0.1.0` is out, or every later release PR proposes `0.1.0` again.
- `@ome-labs/pxe` has its own version, `6.0.0-ome.N` ([pxe-integration.md](pxe-integration.md)), and is published only
  when that version is new.
- Protocol `v0` is frozen from the first release: wire formats, the contract and sealed files change only with a
  migration.

## What a release publishes

| Where                                                      | What                                                                                              |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| npm, `@ome-labs/ome`                                       | `ome`, `omed` and `ome-keeper` launchers; `optionalDependencies` on the three platform packages   |
| npm, `@ome-labs/ome-<platform>`                            | the native `ome` and `ome-keeper` for one platform (`os` and `cpu` set)                           |
| npm, `daemon`, `wallet`, `api`, `wasm`, `contracts`, `pxe` | the libraries, as compiled JavaScript with declarations                                           |
| GitHub release `v<version>`                                | `ome-<version>-<platform>.tar.gz`, `SHA256SUMS`, an SPDX SBOM per archive, `install.sh`           |
| GHCR `ome-labs/ome-relay`, `ome-keeper`                    | multi-architecture images (amd64, arm64) tagged `<version>`, `<major>.<minor>`, `latest`, `sha-…` |
| `ome-labs/homebrew-tap`                                    | `Formula/ome.rb` for the release                                                                  |

Platforms: `darwin-arm64`, `linux-x64` and `linux-arm64`. Linux binaries are built on Ubuntu 22.04, so they need
glibc 2.35 or later.

An archive holds `bin/ome`, `bin/ome-keeper`, `bin/omed` (a shell script that runs the daemon with Node) and
`lib/omed/node_modules` (the daemon and its production dependencies, installed from the release's own npm tarballs on
that platform, since some dependencies are native).

## Install paths

All three end with the same programs; `ome doctor` names the install method and its update command.

- **npm** (`npm i -g @ome-labs/ome`): `bin/ome.js` resolves `@ome-labs/ome-<os>-<arch>` and runs its `ome` with
  `OME_DAEMON` (the daemon's `dist/omed.js`) and `OME_KEEPER_BIN` set. No install scripts download anything.
- **Homebrew** (`brew install ome-labs/tap/ome`): a formula with per-architecture archive URLs and checksums,
  depending on `node@24`. It writes Homebrew's Node into `bin/omed`, so a login service, which has no `PATH`, still
  finds it.
- **Install script** (`scripts/install.sh`, served at `https://ome.network/install.sh` and attached to every release):
  downloads the archive and `SHA256SUMS` from the GitHub release and checks the checksum, and the build attestation
  when `gh` is installed and signed in. Installs into `~/.ome/versions/<version>`, links `~/.ome/current` and
  `~/.ome/bin/{ome,omed,ome-keeper}`, keeps the previous version for a daemon still running from it, and removes older
  ones. Uses a Node 24 or later on `PATH`, otherwise downloads Node 24 from nodejs.org into `~/.ome/runtime` after
  checking its `SHASUMS256.txt`; the Node it settles on is written into `bin/omed`. `OME_VERSION`, `OME_HOME`,
  `--no-modify-path`, and `OME_DOWNLOAD_BASE` (a directory or URL of built archives, for testing) adjust it.

`ome` finds its daemon and keeper in this order: `OME_DAEMON` / `OME_KEEPER_BIN`, beside the running `ome` (symlinks
resolved), `$OME_HOME/bin`, then a repository checkout's build.

macOS: an install by any of the three paths carries no quarantine attribute, so Gatekeeper does not ask. The macOS
binaries are signed with Ome Labs' self-signed code-signing certificate (identifiers `network.ome.ome` and
`network.ome.keeper`). Its designated requirement (`identifier … and certificate leaf = H"…"`) stays the same across
updates, so the keychain keeps letting a new `ome` use the Touch ID sealing key the previous one stored. Gatekeeper
does not trust the certificate; it does not need to. Anyone holding the certificate's key could build a program that
reads members' Touch ID sealing keys without a prompt, so it lives only in the `release` environment's secrets and an
offline backup.

## The pipeline

`.github/workflows/release-please.yml` keeps the release pull request open. Merging it creates the draft release and
its tag `v<version>`, then dispatches `.github/workflows/release.yml` on that tag with `publish` on (a tag pushed with
`GITHUB_TOKEN` would not start a workflow; a dispatch does). Dispatching `release.yml` by hand on `main` is a dry run.

| Job                            | Does                                                                                                                                                                               |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Verify                         | all of `ci.yml` on the commit                                                                                                                                                      |
| npm packages                   | builds the workspace and packs every package (`scripts/packaging/npm.ts`)                                                                                                          |
| Binaries                       | `cargo auditable build --release` of `ome` and `ome-keeper` on each platform's own runner                                                                                          |
| Sign macOS binaries            | signs with the certificate in a temporary keychain; checks the designated requirement (publish only)                                                                               |
| Package                        | platform npm packages and archives (`platform.ts`, `archive.ts`) on each platform                                                                                                  |
| Checksums, SBOMs, attestations | `SHA256SUMS`, syft SBOMs, `install.sh`, the formula; GitHub build attestations (publish only)                                                                                      |
| Install                        | npm, the install script (with and without Node) and Homebrew from the built files, then `scripts/packaging/smoke.ts`                                                               |
| Images, manifests              | per-architecture builds pushed by digest with SBOM and provenance, then tagged and cosign-signed                                                                                   |
| Publish to npm                 | trusted publishing (OIDC, no token; provenance attached); publishes without querying the registry first, and skips a version npm reports as already published                      |
| GitHub release and Homebrew    | uploads the files, publishes the draft, commits the formula to the tap                                                                                                             |
| Published                      | installs the release through the install script and Homebrew and verifies attestations and image signatures; npm is not re-checked, since a new version can take minutes to appear |

Jobs that sign or publish run in the `release` environment, which allows only `v*` tags and needs a maintainer's
approval: once for signing, once for publishing. Every job gets only the permissions it uses; release builds use no
shared caches except the pinned Aztec toolchain.

A broken release is fixed forward: `npm deprecate` the version, mark the GitHub release, and release a patch. npm
versions cannot be reused.

## Verifying a release

```sh
# An archive: its checksum and the build attestation
sha256sum -c --ignore-missing SHA256SUMS
gh attestation verify ome-<version>-<platform>.tar.gz --repo ome-labs/ome

# An npm package: provenance from this repository's release workflow
npm audit signatures            # in a project that installed @ome-labs packages

# An image: signed by this repository's release workflow
cosign verify ghcr.io/ome-labs/ome-relay:<version> \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity https://github.com/ome-labs/ome/.github/workflows/release.yml@refs/tags/v<version>

# The binaries: the dependencies compiled in
cargo audit bin "$(command -v ome)"

# macOS: the signature and its designated requirement
codesign --display --requirements - "$(command -v ome)"
```

## Building a release locally

```sh
pnpm install && pnpm exec turbo run build --filter=@ome-labs/ome...
node scripts/packaging/npm.ts release/npm
scripts/cargo.sh build --release --locked -p ome-cli -p ome-keeper
node scripts/packaging/platform.ts darwin-arm64 crates/target/release release/npm
node scripts/packaging/archive.ts darwin-arm64 crates/target/release release/npm release
(cd release && shasum -a 256 *.tar.gz > SHA256SUMS)
OME_HOME=/tmp/ome-test OME_DOWNLOAD_BASE="$PWD/release" sh scripts/install.sh --version <version> --no-modify-path
```

`release/` is ignored by git.
