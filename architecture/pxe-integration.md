# PXE integration and the wallet

`packages/wallet` (`@ome-labs/wallet`) hosts Aztec's PXE for Ome vaults. Aztec's PXE assumes it holds each account's
incoming viewing secret `ivsk` and uses it for note decryption, tagging and handshakes. An Ome vault's `ivsk` exists
nowhere. Every one of those uses is one multiplication `address_secret · P`, which the keepers compute in shares, so
the wallet adds two things:

- a key store that sends those multiplications to the vault's keepers;
- a minimal patch that makes the PXE ask the key store instead of reading `ivsk`.

Protocol logic is in `@ome-labs/wasm`: combining partials, proposals, the witness and the checks. The wallet keeps
only the Aztec integration: the PXE, keys, the artifact, deployment and the account entrypoint.

## The patch

`patches/@aztec-labs__pxe@6.0.0.patch`, applied by pnpm (`patchedDependencies` in `pnpm-workspace.yaml`) to the
compiled `dest/` files. Each site is marked `// ome:` and guarded by `await this.keyStore.isThresholdAccount?.(…)`.
With the stock `KeyStore` that method is absent, so stock accounts run the stock code unchanged.

| Site                                                           | Stock                                                | For a vault                                                            |
| -------------------------------------------------------------- | ---------------------------------------------------- | ---------------------------------------------------------------------- |
| `createPXE` → `PXE.create` → `openPxeStores`                   | `new KeyStore(store)`                                | `options.createKeyStore(store)` when given                             |
| `utility_execution_oracle#getSharedSecrets`                    | `ivsk` via `getMasterSecretKey(ivpkHash)`, per point | one `addressSecretMul(vault, points, { purpose: "decryption" })`       |
| `pxe#deriveHandshakeSecret`                                    | `ivsk` and the address secret                        | `addressSecretMul(vault, [ephPk], { purpose: "handshake" })`           |
| `log_service#getPointDerivedSecrets` / `#getSecretsForSenders` | recipient `ivsk`, ECDH per sender                    | `addressSecretMul(vault, [senderPoint], { purpose: "tagging" })`       |
| `private_execution_oracle#getAppTaggingSecret`                 | sender `ivsk`, ECDH with the recipient               | `addressSecretMul(vault, [recipientPoint], { purpose: "sender-tag" })` |
| `private_execution_oracle#isUnconstrainedSelfSend`             | forces `address-derived` for every own account       | defers to the wallet's tagging hook                                    |

Each site names its ECDH purpose, which the keepers record in their audit logs and rate limits. `test/pxe-patch.test.ts`
fails if a marker is missing, or if any `getMasterIncomingViewingSecretKey(` call is not guarded.

### Upgrade procedure

On every Aztec release:

1. Run `pnpm patch @aztec-labs/pxe@<new> --edit-dir <dir>`.
2. Reapply each row above. `git apply --recount` of the previous patch, run in that directory, handles moved lines.
   To find sites that moved or were added, search the new `dest/` for:
   - `getMasterIncomingViewingSecretKey` and `getMasterSecretKey`;
   - `computeAddressSecret`, `computeSharedTaggingSecret` and `computeViaEcdh`;
   - `isUnconstrainedSelfSend`.
3. Run `pnpm patch-commit <dir>`.
4. Run the wallet's unit suite, `pnpm --filter @ome-labs/wallet test:e2e` against a local network, and the golden
   vectors (`@ome-labs/vectors`).

## `ThresholdKeyStore`

A PXE key store that also holds vaults.

- **Registration:** a vault is stored like any other account, minus `ivsk_m`, plus a marker.
- **No secret reads:** `getMasterIncomingViewingSecretKey`, `getAccountSecretKeys` and `getMasterSecretKey` for the
  vault's `ivpk` throw `ThresholdViewingError`. The nullifier, outgoing-viewing and tagging secrets are served as
  usual.
- **`addressSecretMul(vault, points, { purpose })`:**
  - batches the points requested in one tick into one call per vault and purpose;
  - requests a point already in flight only once;
  - maps a point that is not on the curve to a random point, so one bad log fails its own decryption, not the sync;
  - rejects the identity, as the stock PXE does.
- **Caches, in memory only:**
  - tagging and sender-tag results last the session;
  - decryption and handshake results go in a 4,096-entry LRU, because the PXE derives the same message secrets on
    every sync, simulation and proof;
  - both are dropped when the vault's provider changes, that is, at a new committee epoch.

### Threshold ECDH

`ThresholdEcdhProvider` asks a `PartialsSource` for partials. It then verifies every DLEQ proof and combines `t_v`
valid partials per point with `@ome-labs/wasm`'s `verifyAndCombine`. The committee comes from the manifest the
member's keeper serves: epoch, preaddress, `Ivpk`, `t_v` and the verification shares. Members whose partials fail
are reported and skipped, and an input with fewer than `t_v` valid partials fails the batch.

| `PartialsSource`          | Where                                                                                   |
| ------------------------- | --------------------------------------------------------------------------------------- |
| `KeeperPartialsSource`    | the member's `ome-keeper` over its local RPC (`ecdh.batch`); it gathers from the others |
| `InProcessPartialsSource` | `@ome-labs/wallet/testing`, shares dealt in-process                                     |

`KeeperClient` is the TypeScript client of the keeper API, typed by `@ome-labs/api`'s method map. The wallet only
connects to a keeper, given its socket and token. Starting and supervising the keeper is the daemon's job.

## Keys and deployment

- **Protocol secrets:** every member holds the vault's nullifier-hiding, outgoing-viewing and tagging secrets and its
  message-signing and fallback public keys. The founder generates them, and the lobby seals them to each member
  (`encodeVaultProtocolSecrets`: a version byte, then seven 32-byte values). They are never published.
- **Public keys:** the vault's `PublicKeys` are those keys plus the DKG's `Ivpk`.
- **`deriveVault(setup)`:**
  1. builds the constructor args;
  2. derives the instance with universal deployment and the agreed salt;
  3. checks Aztec's address equals the address `@ome-labs/wasm` derives from the preaddress and `Ivpk`;
  4. computes both commitments.

  Every founding member derives the same vault.

- **Manifest:** `vaultManifest(deployment, keepers)` is what each member registers with its keeper
  (`vault.register`).
- **Deploy:** `OmeWallet.deployVault` sends the constructor from the member's own account, with the vault in scope
  and no class or instance publication.

## Vault transactions

A vault transaction is an Ome proposal.

1. **Build.** `prepareVaultTx(vault, payload, terms)` maps the Aztec calls to Ome calls (target, selector, kind,
   flags and arguments, all signed) plus `extras`, which carry what the PXE needs but the signatures don't cover:
   - function names and return types;
   - auth witnesses and capsules;
   - extra hashed args;
   - the fee payer.

   `@ome-labs/wasm` builds the proposal and payload, and computes the outer hash in Rust. The wallet also computes
   it with Aztec's own `DefaultAccountEntrypoint` and refuses if the two differ.

2. **Replicate.** The proposer stores the payload and proposal with its keeper (`payloads.put`, `proposals.put`).
   Other members' keepers receive the proposal and fetch the payload by hash.
3. **Check and approve.** Another member's wallet rebuilds the transaction with `vaultTxOf(vault, proposal,
   payload)`. This runs `checkProposal` (the payload is the one referenced and hashes to the outer hash, the config is
   current, the deadline hasn't passed, fees stay off the vault) and Aztec's hash again. The member's
   `UnlockedSigner` approves; the signer runs the same checks itself.
4. **Execute.** `executeVaultTx(tx, approvals)` steps:
   1. refuse approvals collected under an older config version;
   2. tally the approvals with `@ome-labs/wasm`;
   3. once `k` approve, serve the 49-field witness for the outer hash;
   4. send through the vault's entrypoint, with the proposal's nonce pinned and `cancellable` set, so reusing the
      nonce cancels.

**Previews.** `previewVaultTx` simulates a proposal before anyone has approved it. The vault's instance is overridden,
for that simulation only, with the class of `OmeVaultPreview` (`contracts/ome_vault_preview`): the vault's storage
and the functions a proposal can call on it, built from `ome_vault`'s modules, with an entrypoint that accepts any
transaction. The request is built as for Aztec's stub account, whose entrypoint has the same ABI. Calls to the vault
itself (`rotate_membership`, `set_threshold`) run the vault's real checks, so a rotation the contract would refuse is
caught at review. The preview class is never deployed; its test pins its selectors to the vault's.

**Fees.** The vault never pays its own fees. The entrypoint always runs with the `EXTERNAL` fee method; another
method throws. A sponsored fee payer is paid by a call in the payload, so rotation approvals take the fee payers the
member's configuration trusts (`architecture/signer.md`).

**Not supported yet: vault authwits** (another contract acting for the vault through `verify_private_authwit`, as
`transfer_in_private` with an authwit does). Ome's signer signs only checked proposals, never an arbitrary hash, so
an authwit needs its own proposal kind first.

## Tagging, and what a removed member still sees

Aztec finds a recipient's notes by tags derived from a secret the sender and recipient share. For a vault there are
three kinds:

| Messages                                                         | Secret                                              | After a member is removed                                                                         |
| ---------------------------------------------------------------- | --------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| the vault's unconstrained self-sends (change notes, for example) | **the epoch tagging secret** (below)                | new self-sends use a secret the removed member can't derive                                       |
| from a counterparty (address-derived)                            | `address_secret · sender_point`, fixed for the pair | the removed member keeps the secret: it can see _that_ that counterparty paid the vault, and when |
| after a handshake                                                | the handshake's secret, fixed until a new handshake | the same, until the counterparty handshakes again                                                 |
| constrained self-sends (config notes, for example)               | address-derived, fixed                              | the removed member can see that the vault wrote one, and when                                     |

**Epoch tagging secret.** The vault's unconstrained self-sends are tagged with
`epochTaggingSecret(vault, epoch, transcript)` from `ome-protocol`. It is `s · G`, where `s` is a Poseidon2 hash,
tagged `ome.epoch-tagging.v0`, of the vault, the committee epoch and that epoch's ceremony transcript (the founding
DKG, then each reshare).

- Only the epoch's committee takes part in its ceremony, so a member removed earlier never sees the transcript.
- Reshare dealers must therefore be members who stay.
- The wallet's tagging hook returns the secret as an `arbitrary-secret` strategy, and `setViewing` registers it as a
  tagging source. Earlier epochs' secrets stay registered, so their notes are still found.
- A member who joins later receives the earlier epochs' transcripts in the reshare's sealed hand-off
  (`architecture/daemon.md`), and `addEpochSecret` registers each. Without them, the joiner would miss the vault's
  older change notes, which hold its balance.

**Content and amounts.** A removed member can't read anything that arrives after removal. Every note's decryption
needs `address_secret · ephPk` for a fresh ephemeral key, and only the new committee can produce it. What leaks is
metadata: who sent to the vault, and when, for counterparties whose secret the member knew. After a rotation that
removed members, the daemon lists those counterparties as exposed (`vault.counterparties`) and emits
`handshakeAdvised`, so the member can ask them to handshake with the vault again; until a counterparty does, that
metadata leak stays (`architecture/daemon.md`).

## Code and tests

| File                               | What it checks                                                                       |
| ---------------------------------- | ------------------------------------------------------------------------------------ |
| `test/pxe-patch.test.ts`           | every site is marked, the factory is passed through, and no `ivsk` read is unguarded |
| `test/threshold-key-store.test.ts` | the key store, below; and the provider combining partials                            |
| `test/deployment.test.ts`          | deployment; the protocol secrets encoding, below                                     |
| `test/approvals.test.ts`           | payload hashing and witness assembly, below                                          |
| `test/e2e/vault.test.ts`           | the local-network suite, below                                                       |

- **Key store:** vaults are registered without `ivsk`, every read of it throws, and ordinary accounts are unaffected.
  Batching, de-duplication and caching work per purpose, the caches drop at a new epoch, and off-curve and identity
  inputs are handled.
- **Provider:** any `t_v` members give the same result, a corrupt member is named, and too few valid partials fail.
- **Deployment:** every member derives the same address, which matches Aztec's and the address context. The
  manifest and config are accepted by the bindings. The address depends on each input, and invalid setups are
  refused.
- **Protocol secrets:** they round-trip, and malformed bytes or off-curve points are refused.
- **Payload hashing:** Aztec's entrypoint and the Ome proposal hash a real payload (a transfer and a sponsor call,
  with an auth witness and a fee payer) identically, and the payload round-trips. Utility calls are refused.
- **Witness:** approvals tally in slot order with the empty slot zeroed. The witness provider serves only approved
  hashes.

**End to end:** `pnpm --filter @ome-labs/wallet test:e2e`, against `aztec start --local-network`. Three real
`ome-keeper` processes run the DKG, and two members' wallets each use their own keeper. The suite checks that:

- both wallets agree with the keepers on the deployed config;
- both find the vault's notes through threshold ECDH;
- a transfer previews, and so does a `rotate_membership` proposal (daemon suite);
- a proposal travels between keepers, is checked in the other wallet, and pays with two approvals;
- one approval, a late proposal and a cancelled nonce are refused;
- a reshare and a `rotate_membership` proposal (which pays its sponsored fee) take member 3 out:
  - the new committee activates on all three of its keepers;
  - member 3's keeper, which couldn't verify the new committee, alarms and is retired;
  - stale approvals are refused;
  - the newcomer's approval counts;
  - both wallets keep reading the vault in the new epoch.

Measured on Apple Silicon with fake proofs: 8 tests in 33 s, setup included. `OME_E2E_PROVE=1` proves client-side.
