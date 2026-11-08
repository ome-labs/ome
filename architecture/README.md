# Architecture

Ome is a private k-of-n multisig on Aztec with threshold viewing: `k` member signatures authorize spending, and the
vault's incoming viewing secret is split so that `t_v` members' keepers are needed to read it. This folder records
verified decisions about the components that exist; each component gets its own page when it lands.

## Components in the tree

| Path                    | What it is                                                                                                                                  |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `contracts/ome_vault`   | `OmeVault`, the vault's Aztec account contract (Aztec.nr 6.0.0), with its unit, integration and end-to-end tests under `src/test/`          |
| `contracts/test_signer` | Test-only contract that returns TXE Schnorr signatures; never deployed                                                                      |
| `crates/crypto`         | `ome-crypto`: Grumpkin, Poseidon2, Aztec key derivation, Schnorr, DKG, threshold ECDH, resharing and HPKE sealing ([crypto.md](crypto.md))  |
| `crates/wire`           | `ome-wire`: canonical, versioned, signed messages between keepers, daemons and relays ([wire.md](wire.md))                                  |
| `crates/protocol`       | `ome-protocol`: the vault's rules: contract mirror, payload hashing, approvals, tallies, rotations and lobbies ([protocol.md](protocol.md)) |
| `crates/signer`         | `ome-signer`: the member's approval key, sealed at rest and signing only proposals `ome-protocol` accepts ([signer.md](signer.md))          |
| `crates/net`            | `ome-net`: transport, libp2p node, framing, admission and relay mailboxes ([networking.md](networking.md))                                  |
| `tools/vectors`         | `@ome-labs/vectors`: golden vectors computed with Aztec 6.0.0's own packages                                                                |
| `scripts/`              | Repository scripts: guarded cargo runs, the WebAssembly build check, the contract's failure-message check and gate flamegraphs              |

The TypeScript packages (`packages/`) are empty; crates and packages join as they are built.

## The vault contract

`OmeVault` holds the spending policy in private notes owned by the vault:

- a **config note**: spending threshold `k`, signer count `n`, a config `version`, a commitment to the ordered signer
  keys, the viewing threshold `t_v` and a commitment to the viewing committee's verification shares;
- one **signer note** per member, so members can rebuild the ordered signer set (authorization never reads them).

| Function                                          | Kind                | What it does                                                                                           |
| ------------------------------------------------- | ------------------- | ------------------------------------------------------------------------------------------------------ |
| `constructor`                                     | private initializer | validates the signer set, `1 ≤ k ≤ n`, `2 ≤ t_v ≤ n` and the committee; stores the config at version 0 |
| `entrypoint`                                      | private             | Aztec's account entrypoint; authorizes with `is_authorized` (below), then runs the payload             |
| `verify_private_authwit`                          | private, view       | authwit check for other contracts acting for the vault, through the same authorization                 |
| `set_threshold`                                   | private, only self  | new `k`; bumps the version                                                                             |
| `invalidate_pending`                              | private, only self  | bumps the version only, voiding every collected approval                                               |
| `rotate_membership`                               | private, only self  | new signer set, thresholds and committee in one step; fewer than the current `t_v` members may leave   |
| `get_config`, `get_signer_set`, `lookup_validity` | utility             | reads for members' wallets                                                                             |

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

| Function                 |  Gates |
| ------------------------ | -----: |
| `verify_private_authwit` | 18,665 |
| `constructor`            | 25,351 |
| `entrypoint`             | 27,176 |
| `invalidate_pending`     | 37,385 |
| `set_threshold`          | 37,443 |
| `rotate_membership`      | 85,734 |

Known limit: `rotate_membership` checks that new committee shares are well formed but not that they are shares of the
vault's viewing key. Approvers' software checks it before signing (`ome-protocol`'s `check_rotation`, see
[protocol.md](protocol.md)); enforcing it in the contract belongs to the contract track.

## Compatibility with Aztec

Ome's cryptography must produce exactly Aztec's bytes. `tools/vectors` computes reference values with Aztec 6.0.0's
own packages; CI fails when a committed vector file differs from what the pinned Aztec produces. The contract's
`ome.*.v0` vectors are pinned in both the Noir tests and `crates/protocol/tests/vectors/contract.json`.

## Planned components

Not built yet; each will get a page here when it lands: `ome-keeper`, `ome-relay`, `ome-api`, `ome-client`, `ome-wasm`, `ome-ffi`, `@ome-labs/wallet` and
`@ome-labs/daemon`.
