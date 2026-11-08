# Bindings

The Rust core (`ome-crypto`, `ome-protocol`, `ome-signer`) reaches other languages through bindings:

- **WebAssembly** (`crates/wasm`, `@ome-labs/wasm`) for TypeScript, in Node (the daemon, the wallet, an SDK) and in
  the browser;
- **FFI** (`ome-ffi`, phase 13) for Swift and Kotlin, not built yet.

Bindings convert values and errors; they hold no protocol logic. What a binding exposes is tested at the boundary,
and the logic itself is tested in Rust.

## WebAssembly

`@ome-labs/wasm` replaces Clave's `clave-napi`, which needed one native build per platform and did not run in a
browser. It is one module for every platform.

### Surface

| Area              | Exports                                                                                                                               |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Threshold reading | `preaddress`, `addressContext`, `addressPublicShares`, `verifyAndCombine`                                                             |
| Contract mirror   | `approvalMessage`, `rejectionMessage`, `signerSetCommitment`, `committeeCommitment`, `encodeWitness`                                  |
| Payloads          | `outerHash`, `appPayloadHash`, `selector`                                                                                             |
| Proposals         | `buildProposal`, `checkProposal`, `decodeProposal`, `decodeApproval`, `decodeRejection`, `verifyApproval`, `verifyRejection`, `tally` |
| Signer            | `generateSigner`, `sealedPublicKey`, `sealingKey`, and `UnlockedSigner` (`open`, `publicKey`, `approve`, `approveRotation`, `reject`) |
| Membership        | `verifyRotationShares`, `checkRotation`, `rotationCall`, `policyWarnings`                                                             |

- **`verifyAndCombine`** takes the keeper's `ecdh.batch` responses as they come back, with the committee's
  verification shares. It returns `address_secret · P` for every input, and names any member whose partials were
  malformed or failed their DLEQ proof. It throws if an input has fewer than `t_v` valid partials.
- **Proposals** take bodies as hex of their canonical `ome-wire` encoding, the form `proposals.put`, `payloads.put`,
  `approvals.put` and `rejections.put` store. A vault is `{ manifest, config }`: the keeper's manifest and the
  config note read from the chain. The bindings check one against the other with `Vault::from_manifest`, so a
  proposal is never checked against an inconsistent vault.
- **`tally`** counts each slot's first valid vote, lists the votes it refused, and returns the 49-field auth witness
  once `k` members approved.

### The signing key stays in WebAssembly memory

- `generateSigner(secret)` creates a key and returns only the sealed JSON document.
- `UnlockedSigner.open(sealed, secret)` returns an object whose key lives in the module's memory. It signs only
  through `ome-signer`'s `approve`, `approve_rotation` and `reject`, which run `ome-protocol`'s checks first.
- `free()` (or `using`, through `Symbol.dispose`) drops the key, and `SigningKey` zeroizes on drop.
- The daemon's unlock window is one `UnlockedSigner`, so Argon2id runs once per unlock, not once per vote.
- A secret is `{ passphrase }` (Argon2id 64 MiB × 3) or `{ key }`, the 32-byte sealing key `sealingKey` derives. A
  platform keychain can hold that key, and opening with it skips Argon2id.

### Values and errors

- Params and results are plain objects in the `@ome-labs/api` shapes: `0x` hex (`Hex32`, `Hex64` for signatures),
  `{ x, y }` points, and `null` for a missing value.
- Points are checked to be on the curve, and fields to be canonical, on the way in. This uses `ome-api`'s `core`
  feature, the same conversions the keeper RPC uses.
- Times are Unix seconds as JavaScript numbers. A value above 2^53, or one that is not a whole number, is refused;
  deadlines are near-term timestamps, so this costs nothing in practice.
- Every failure throws an `Error` named `OmeWasmError`, with `kind`:
  - `invalid` for a malformed or refused input;
  - `auth` for a wrong passphrase or key;
  - `failed` for anything else.

### Types

- The boundary shapes are Rust types in `src/types.rs`, rendered with ts-rs into `src/types.d.ts`
  (`pnpm --filter @ome-labs/wasm generate`). They import the shared shapes from `@ome-labs/api`.
- The module embeds that file in its own declarations (a `typescript_custom_section`), and every export names its
  param and result types (`unchecked_param_type`, `unchecked_return_type`).
- The API types CI job regenerates the file and fails on any diff.

### Build and packaging

`scripts/build.ts` compiles `ome-wasm` for `wasm32-unknown-unknown` with the `wasm` profile (release with full LTO,
opt-level 3 for Argon2id and point arithmetic). It builds twice, without and with the `testing` feature, and runs the
`wasm-bindgen` CLI for two targets:

| Path                      | For                                                                                      |
| ------------------------- | ---------------------------------------------------------------------------------------- |
| `dist/main/node`          | Node: CommonJS, loads synchronously (marked `"type": "commonjs"` inside the ESM package) |
| `dist/main/web`           | Browsers and bundlers: an ES module; call `await init()` (or `initSync`) first           |
| `dist/testing/{node,web}` | `@ome-labs/wasm/testing` only                                                            |

`package.json` maps `.` and `./testing` through `browser`, `node` and `default` export conditions, each with its
typings.

- **Randomness**: key generation, sealing and signing use `getrandom`'s `wasm_js` backend, which is
  `crypto.getRandomValues` in browsers and in Node 19 and later. The backend is enabled only in `ome-wasm`, so the
  core crates stay platform-neutral.
- **Tooling**: the plain `wasm-bindgen` CLI. `wasm-pack` is maintained again (it moved to the wasm-bindgen
  organization), but it builds one target per run and writes a `package.json` we would discard.
- **Version pin**: libp2p pins `wasm-bindgen-futures` 0.4.58, which fixes `wasm-bindgen` at 0.2.108 for the whole
  Cargo workspace. The CLI must be the same version; the build script checks this and names the install command.
- **Size**: the module is 655 KB.

### Testing helpers

`@ome-labs/wasm/testing` is built from the `testing` feature and is never in the default build. It handles raw
secrets, which production code never does:

- `dealShares`: splits a secret into viewing shares;
- `partials`: a member's ECDH answer;
- `publicKey` and `sign`: raw-key Schnorr.

### Tests

- `tests/node.test.ts`, through the package as a consumer imports it:
  - any `t_v` members' partials combine to the same points;
  - a forged partial names its member, and too few valid partials throw;
  - a proposal builds, checks, is approved and rejected, and tallies into the witness, with a foreign approval
    refused and an expired proposal reported;
  - the signer refuses expired or mismatched proposals;
  - a passphrase seals and opens, the sealing key opens, and a wrong passphrase is `auth`;
  - a reshared committee passes the rotation checks, and its `rotate_membership` call is built;
  - unrelated shares and unrefreshed shares are refused;
  - threshold warnings;
  - malformed input throws `OmeWasmError`.
- `tests/parity.test.ts`: the results equal the golden vectors from Aztec 6.0.0:
  - signer-set and committee commitments;
  - approval messages (those whose deadline a number can hold);
  - selectors, payload hashes and outer hashes;
  - Aztec preaddresses and addresses;
  - Aztec's ECDH shared point, from partials of shares dealt from each account's `ivsk`.
- `tests/web.test.ts`: the browser build initializes from its bytes and generates a signer.

### Timings

Measured with `pnpm --filter @ome-labs/wasm bench` (Node 24, Apple Silicon), against the same paths in native release
Rust on the same machine:

| Path                                     | WebAssembly | Native  |
| ---------------------------------------- | ----------- | ------- |
| verify and combine, `t_v = 2`, per point | 1.38 ms     | 0.56 ms |
| verify and combine, 1,000 points         | 1.38 s      | 0.56 s  |
| one partial with its proof (keeper side) | 1.06 ms     | 0.26 ms |
| outer hash, 5 calls                      | 0.51 ms     | —       |
| unseal, Argon2id 64 MiB × 3              | 97–111 ms   | 70 ms   |
| unseal with the sealing key              | 0.28 ms     | —       |

Combining is about 2.5 times slower than native. Reading a thousand notes takes about 1.4 s, so the narrow napi
fallback the plan allows is not needed. Partials run in the keeper, which is native, so their wasm cost matters only
in tests.
