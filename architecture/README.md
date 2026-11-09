# Architecture

Ome is a private k-of-n multisig on Aztec with threshold viewing: `k` member signatures authorize spending, and the
vault's incoming viewing secret is split so that `t_v` members' keepers are needed to read it. This folder records
verified decisions about the components that exist; each component gets its own page when it lands.

## Components in the tree

| Path                    | What it is                                                                                                                                                                 |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `contracts/ome_vault`   | `OmeVault`, the vault's Aztec account contract (Aztec.nr 6.0.0), with its unit, integration and end-to-end tests under `src/test/`                                         |
| `contracts/test_signer` | Test-only contract that returns TXE Schnorr signatures; never deployed                                                                                                     |
| `crates/crypto`         | `ome-crypto`: Grumpkin, Poseidon2, Aztec key derivation, Schnorr, DKG, threshold ECDH, resharing and HPKE sealing ([crypto.md](crypto.md))                                 |
| `crates/wire`           | `ome-wire`: canonical, versioned, signed messages between keepers, daemons and relays ([wire.md](wire.md))                                                                 |
| `crates/protocol`       | `ome-protocol`: the vault's rules: contract mirror, payload hashing, approvals, tallies, rotations and lobbies ([protocol.md](protocol.md))                                |
| `crates/signer`         | `ome-signer`: the member's approval key, sealed at rest and signing only proposals `ome-protocol` accepts ([signer.md](signer.md))                                         |
| `crates/net`            | `ome-net`: transport, libp2p node, framing, admission and relay mailboxes ([networking.md](networking.md))                                                                 |
| `crates/keeper`         | `ome-keeper`: viewing-share custody, ceremonies, threshold ECDH, replication, lobbies, mailboxes, the local RPC and the `ome-keeper` binary ([keeper.md](keeper.md))       |
| `crates/api`            | `ome-api`: every method, event and error of the keeper and daemon APIs, and their TypeScript ([api.md](api.md))                                                            |
| `crates/client`         | `ome-client`: typed keeper and daemon clients and daemon discovery ([api.md](api.md))                                                                                      |
| `crates/wasm`           | `ome-wasm` / `@ome-labs/wasm`: the core for TypeScript as WebAssembly, for Node and the browser ([bindings.md](bindings.md))                                               |
| `crates/ffi`            | `ome-ffi`: the phone core of a member's signing device, with Swift and Kotlin bindings ([bindings.md](bindings.md), [mobile.md](mobile.md))                                |
| `crates/cli`            | `ome-cli`: `ome`, a member's command line, a client of their daemon ([cli.md](cli.md))                                                                                     |
| `crates/relay`          | `ome-relay`: the untrusted circuit relay and sealed mailbox store, its binary and the `ome-node` image ([relay.md](relay.md))                                              |
| `packages/daemon`       | `@ome-labs/daemon`: `omed`, a member's service: the API server, the sealed signer, keeper supervision, founding, proposals and membership changes ([daemon.md](daemon.md)) |
| `packages/wallet`       | `@ome-labs/wallet`: Aztec's PXE for Ome vaults, with the threshold key store, deployment and proposals ([pxe-integration.md](pxe-integration.md))                          |
| `patches/`              | The guarded `@aztec-labs/pxe` 6.0.0 patch the wallet needs ([pxe-integration.md](pxe-integration.md))                                                                      |
| `packages/api`          | `@ome-labs/api`: the API's TypeScript declarations, generated from `crates/api`                                                                                            |
| `tools/vectors`         | `@ome-labs/vectors`: golden vectors computed with Aztec 6.0.0's own packages                                                                                               |
| `scripts/`              | Repository scripts: guarded cargo runs, the WebAssembly build check, the contract's failure-message check and gate flamegraphs                                             |

Crates and packages join as they are built.

## The vault contract

`OmeVault` holds the spending policy in private notes owned by the vault:

- a **config note**: spending threshold `k`, signer count `n`, a config `version`, a commitment to the ordered signer
  keys, the viewing threshold `t_v` and a commitment to the viewing committee's verification shares;
- one **signer note** per member, so members can rebuild the ordered signer set (authorization never reads them).

| Function                                          | Kind                | What it does                                                                                                                                             |
| ------------------------------------------------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `constructor`                                     | private initializer | validates the signer set, `1 ≤ k ≤ n`, `2 ≤ t_v ≤ n` and the committee; stores the config at version 0                                                   |
| `entrypoint`                                      | private             | Aztec's account entrypoint; authorizes with `is_authorized` (below), then runs the payload                                                               |
| `verify_private_authwit`                          | private, view       | authwit check for other contracts acting for the vault, through the same authorization                                                                   |
| `set_threshold`                                   | private, only self  | new `k`; bumps the version                                                                                                                               |
| `invalidate_pending`                              | private, only self  | bumps the version only, voiding every collected approval                                                                                                 |
| `rotate_membership`                               | private, only self  | new signer set, thresholds and committee in one step; fewer than the current `t_v` members may leave; the committee must hold the vault's `Ivpk` (below) |
| `get_config`, `get_signer_set`, `lookup_validity` | utility             | reads for members' wallets                                                                                                                               |

**Authorization** (`is_authorized`) reads the config note without nullifying it, so it also holds inside static
calls. It decodes a 49-field auth witness (8 signer keys, 8 Schnorr signatures as 4 limbs each, a deadline), requires
the keys to match the stored commitment, checks the deadline against the anchor block, and counts valid signatures
over

```text
Poseidon2("ome.approval.v0", [vault, outer_hash, config_version, expires_at])
```

until at least `k`. All 8 slots are verified so every vault size proves the same circuit. The transaction's
expiration is set to the earlier of the deadline and the anchor timestamp plus `CONFIG_MAX_AGE` (3,600 s), which bounds
how long a superseded config can still authorize.

**What is public:** nothing about the members or thresholds; they live in private notes and the witness. The
transaction expiration timestamp is public.

**Domain tags** (wire format shared with every encoder, pinned by tests): `ome.approval.v0`, `ome.signers.v0`,
`ome.committee.v0`.

Gate counts (`pnpm --filter @ome-labs/contracts profile`):

| Function                 |   Gates |
| ------------------------ | ------: |
| `verify_private_authwit` |  18,665 |
| `constructor`            |  25,351 |
| `entrypoint`             |  27,176 |
| `invalidate_pending`     |  37,385 |
| `set_threshold`          |  37,443 |
| `rotate_membership`      | 100,552 |

**A rotation's committee must hold the vault's key.** `rotate_membership` also takes the committee polynomial's
Feldman commitments `C_0 … C_{t_v-1}` (zero-padded to 8; `ome-protocol`'s `membership::polynomial` derives them from
the shares). The contract asserts `C_0` is the vault's own `ivpk_m` (`get_public_keys(self.address)`, constrained by
the address preimage), that exactly `C_0 … C_{t_v-1}` are present (degree `t_v - 1`, so the threshold cannot silently
drop), and that every share `Y_j = Σ_k j^k · C_k`. Then any `t_v` members can read for the vault, and no set of
approvals can install a committee that cannot. Each check is a few native Grumpkin additions (the member index is a
constant), about 15,000 gates in all. Approvers still check off chain first (`check_rotation`), including that the
shares were refreshed, which the contract cannot see.

## Compatibility with Aztec

Ome's cryptography must produce exactly Aztec's bytes. `tools/vectors` computes reference values with Aztec 6.0.0's
own packages; CI fails when a committed vector file differs from what the pinned Aztec produces. The contract's
`ome.*.v0` vectors are pinned in both the Noir tests and `crates/protocol/tests/vectors/contract.json`.

## Measurements

Apple Silicon, Aztec 6.0.0. Local network numbers are from `pnpm bench:proving` (real client-side proofs; the local
network's own blocks are instant), Testnet numbers from `pnpm drill:testnet`, relay numbers from `pnpm test:network`.

| What                                           | Local network, proven | Testnet, proven |
| ---------------------------------------------- | --------------------: | --------------: |
| found a 2-of-3 vault (DKG, deployment)         |                 7.7 s |           102 s |
| deploy a token and mint to the vault           |                12.2 s |           147 s |
| 2-of-3 payment: rebuild, prove, send, mined    |                 6.5 s |            55 s |
| remove a member (reshare, `rotate_membership`) |                 6.2 s |            73 s |
| every member reads the vault's balance         |                     — |            13 s |

The Testnet run shared the machine with a Docker build, so its times are upper bounds; most of them are block
inclusion, not proving.

| Threshold and ceremony path                       |   Time |
| ------------------------------------------------- | -----: |
| threshold read, 1 point, direct (loopback)        |  19 ms |
| threshold read, 100 points, direct                |  1.4 s |
| threshold read, 1 point, through a relay (Docker) |  73 ms |
| DKG, 3 keepers, `t_v = 2`, direct                 | 0.52 s |
| DKG, 8 keepers, `t_v = 5`, direct                 | 0.55 s |
| DKG, 3 keepers, through a relay                   |  1.5 s |
| a stopped keeper catches up after restart (relay) |  1.2 s |

Relay timings include a `docker exec` per call.

## Planned components

None: the mobile apps will build on `ome-ffi`.
