# ome-protocol

`crates/protocol` holds the rules of an Ome vault in one place. The keeper, the daemon (through WebAssembly), a mobile
approver and future apps all decide the same way. It builds on `ome-crypto` (primitives) and `ome-wire` (message
formats). It is pure: there is no I/O, and callers pass in the current time. It builds for `wasm32-unknown-unknown`.

## Contract mirror (`vault`)

Every value here equals what `OmeVault` computes. `tests/contract_vectors.rs` loads
`tests/vectors/contract.json`, and the Noir tests pin the same vectors, so a change on either side fails both.

| Value                    | Definition                                                                                    |
| ------------------------ | --------------------------------------------------------------------------------------------- |
| domain tag               | the field whose big-endian bytes spell the ASCII name                                         |
| signer-set commitment    | `Poseidon2("ome.signers.v0", [x₀, y₀, …, x₇, y₇])`, empty slots zero                          |
| committee commitment     | `Poseidon2("ome.committee.v0", [t_v, Y₀.x, Y₀.y, …, Y₇.y])`, empty slots zero                 |
| approval message         | `Poseidon2("ome.approval.v0", [vault, outer_hash, config_version, expires_at])`               |
| rejection message        | `Poseidon2("ome.rejection.v0", [vault, id_hi, id_lo])` (off chain only)                       |
| auth witness (49 fields) | keys `[0, 16)`, one 4-limb signature per slot `[16, 48)` (zero = no vote), `expires_at` at 48 |

`SignerSet` and `Committee` require 1–8 distinct points. `Committee` also requires `2 ≤ t_v ≤ n`.

`Config` mirrors the config note, and its transitions mirror `governance.nr`:

- `genesis`;
- `bump_version` (`invalidate_pending`);
- `with_threshold` (`set_threshold`, with `1 ≤ k ≤ n`);
- `rotate` (`rotate_membership`): the current set must match the stored commitment, at most `t_v − 1` members may
  leave, and the thresholds must be in range.

Every transition increments the version.

## What members sign (`payload`)

A vault transaction runs through Aztec's account entrypoint, which authorizes one hash. `ome-protocol` recomputes it
from the calls:

```text
args_hash(private call) = 0 if no args, else Poseidon2(FUNCTION_ARGS, args)
args_hash(public call)  = Poseidon2(PUBLIC_CALLDATA, [selector, …args])
call fields             = [args_hash, selector, target, is_public, hide_msg_sender, is_static]
app payload             = 5 calls × 6 fields (padded) ‖ tx_nonce          (31 fields)
payload hash            = Poseidon2(SIGNATURE_PAYLOAD, app payload)
outer hash              = Poseidon2(AUTHWIT_OUTER, [vault, chain_id, rollup_version, payload hash])
selector                = low 4 bytes of Poseidon2(signature bytes, 31 per field, little-endian)
```

Padding slots are Aztec's `FunctionCall.empty()`: a public call with selector 0 and no arguments. Its args hash is
therefore `Poseidon2(PUBLIC_CALLDATA, [0])`, not zero. The domain separators are Aztec 6.0.0's `DomainSeparator`
values. `tests/payload.rs` checks every step against `tests/vectors/payload.json`, which `tools/vectors` computes
with `@aztec-labs/entrypoints` 6.0.0. The checks cover selectors, args and calldata hashes, the 31 fields, the
payload hash and the outer hash, for 0, 1, 3 and 5 calls.

A proposal payload is `canonical(ExecutionRequest { calls, extras })`. Each `Call` carries:

- the target;
- the selector;
- private or public;
- the static and hide-sender flags;
- the arguments;
- optionally its function signature, which must hash to the selector.

`arguments(signature, args)` splits a call's arguments by its signature's parameter types for display: one field per
`Field`, integer or `bool`, the members of a tuple or struct in order, `N` times an array element's width for
`[T;N]`, and `N` for `str<N>`. A type it doesn't know, or a count that doesn't add up, is
`UndecodableArguments`.

Calls the entrypoint would mishandle are refused:

- a zero target (silently skipped);
- `hide_msg_sender` on a private call (reverts);
- non-canonical fields;
- more than five calls.

`extras` holds the PXE's execution data that the signature does not cover: auth witnesses, capsules and extra hashed
arguments. It cannot change which calls run or with what arguments. Contracts must constrain any hint they read
from it.

## Proposals (`proposals`)

`Draft::build` produces the wire `Proposal` and its `ProposalPayload`, with the outer hash computed from the calls.

`check_proposal` runs before a member reviews or signs anything. It returns the decoded calls to show the member,
after checking that:

1. the payload is the one the proposal references, and it decodes;
2. the calls hash to the proposal's `outer_hash` on this vault and chain;
3. the proposal is for the current config version;
4. its deadline has not passed;
5. the proposer is a member;
6. the fee method is `EXTERNAL`. The vault's other fee methods would make it the public fee payer, which breaks the
   invariant that fees are never paid from the vault in public.

`verify_approval` checks that an approval is for this proposal's id and that it is a valid Schnorr signature by the
key in its slot. The signature covers the approval message, which binds the vault, outer hash, config version and
deadline. An approval copied to another proposal, vault, version, deadline or slot fails. `verify_rejection` does the
same with the rejection message.

`Tally` records the first valid vote from each slot and ignores later ones. Its status is checked in this order:

1. `Expired` if `now > expires_at`;
2. `Approved` with at least `k` approvals;
3. `Blocked` with more than `n − k` rejections (`k` can no longer be reached);
4. otherwise `Pending`.

Once approved, `witness()` builds the auth witness from the `k` lowest-slot approvals. The result is identical
whichever member builds it.

## Membership (`membership`)

`Vault::from_manifest(manifest, config)` is the only way to obtain a `Vault`, so a keeper or client acts only on a
manifest that is the vault's actual committee. It checks that:

1. the manifest is well formed, and its address follows from its preaddress and group key;
2. its signer keys and count match the stored signer-set commitment;
3. its threshold matches the config;
4. its verification shares and `t_v` match the stored committee commitment;
5. its epoch is not newer than the config version;
6. its shares belong to the group key (`verify_shares`, below).

`verify_shares(Ivpk, committee)` interpolates the committee's verification shares with `Commitments::interpolate`.
It requires every share to lie on one polynomial of degree `t_v − 1` and that polynomial's constant term to equal
`Ivpk`. Errors:

- `ShareOffPolynomial(i)` names the first share off the polynomial;
- `GroupKeyMismatch` when the polynomial belongs to another key.

`polynomial(committee)` returns that polynomial's Feldman commitments `C_0 … C_{t_v-1}`, interpolated from the first
`t_v` shares. `Rotation::to_call` passes them as `rotate_membership`'s last argument, and the contract checks every
share against them and `C_0` against the vault's `ivpk_m`, so the property `verify_shares` checks off chain also holds
on chain. `Rotation::from_call` refuses a call whose polynomial is not the shares' own.

`check_rotation(vault, rotation)` runs before an approver signs a `rotate_membership`, and refuses rather than warns:

1. the contract's own transition (`Config::rotate`);
2. `verify_shares` for the new committee against the vault's `Ivpk`. The contract checks only that the shares are
   well formed, so this closes that gap for approvers;
3. no new share equals a current one. A rotation that keeps the old shares would leave removed members able to
   decrypt.

**Signer swaps** move one member's signing key to another device (`architecture/mobile.md`):

- `Rotation::signer_swap(vault, slot, key)` builds the `rotate_membership` that replaces one slot's key and keeps the
  committee and both thresholds.
- `check_signer_swap(vault, rotation)` runs before an approver signs one. It requires the committee, `k` and the
  member count unchanged, exactly one key replaced by a key that is not a signer yet (`NotASignerSwap` otherwise),
  and the contract's own transition. It returns the swapped slot and the next config.
- `swapped_manifest(manifest, config, calls)` is how a keeper adopts the new key. One of an executed proposal's
  calls must be a signer swap of the manifest (its current keys the manifest's, its committee and thresholds
  unchanged), and the manifest with that key replaced must pass `Vault::from_manifest` against the config read after
  it ran. The epoch stays: the committee did not change.

`policy::warnings(k, t_v, n)` reports configurations the contract allows but members should choose knowingly:

| Warning         | Condition                            |
| --------------- | ------------------------------------ |
| `SingleSigner`  | `k = 1` with more than one member    |
| `NoSpareSigner` | `k = n` with more than one member    |
| `TwoCanRead`    | `t_v = 2` with three or more members |
| `NoSpareViewer` | `t_v = n`                            |

`epoch_tagging_secret(vault, epoch, transcript)` is the tagging secret for the vault's own unconstrained notes in a
committee epoch: `s · G`, where `s = Poseidon2("ome.epoch-tagging.v0", [vault, epoch, t_0, t_1, t_2, t_3])` and
`t_i` are the 16-byte big-endian limbs of the epoch's 64-byte ceremony transcript (the founding DKG, then each
reshare). Only the epoch's committee takes part in that ceremony, so a member removed earlier can't derive later
epochs' secrets, provided reshare dealers are members who stay. The wallet uses it
(`architecture/pxe-integration.md`).

## Lobbies (`lobby`)

- **Join proof**: `HMAC-SHA256(invite secret, "ome.lobby.join.v0" ‖ lobby ‖ identity ‖ transport)`, compared in
  constant time. It binds the join to the joiner's keeper keys, and `verify_join` also requires the joiner to be the
  signer of the message carrying it.
- **Founding secrets**: sealed with an HPKE envelope of kind 4, bound to the plan's session, epoch 0, the host (index
  1. and the recipient's index.
- **Version**: `plan_version` returns the newest version that this build and every founder's keeper speak. It fails
  with `NoCommonVersion` otherwise.

## Tags

| Tag                    | Used for                         |
| ---------------------- | -------------------------------- |
| `ome.approval.v0`      | approval message (contract)      |
| `ome.signers.v0`       | signer-set commitment (contract) |
| `ome.committee.v0`     | committee commitment (contract)  |
| `ome.rejection.v0`     | rejection message                |
| `ome.lobby.join.v0`    | join proof                       |
| `ome.epoch-tagging.v0` | epoch tagging secret             |
