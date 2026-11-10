# The daemon (`omed`)

`packages/daemon` (`@ome-labs/daemon`) is one member's background service for one profile. Every app (the CLI, a
desktop app) talks to it through the daemon API of `@ome-labs/api` (`architecture/api.md`).

It does five things:

- serves the API on an owner-only socket;
- keeps the sealed signer;
- runs and supervises the member's keeper;
- hosts the wallet's PXE (`architecture/pxe-integration.md`);
- founds vaults, runs proposals and membership changes, and tells the keeper what the chain says.

It runs under Node 24, straight from its TypeScript sources: `omed [--profile <name>] [--home <dir>]`.

## Profile and files

The Ome home is `OME_HOME`, or `~/.ome`. Each profile is `profiles/<name>/`. The name is lowercase letters, digits,
`-` and `_`, so it can't leave the home. The profile directory is owner-only (`0700`), and every file in it is written
`0600` and atomically.

| File                          | Holds                                                                                                          |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `profile.json`                | the public profile (name, signer key, account), settings, vault aliases, the default vault, tracked tokens     |
| `signer.json`                 | the signing key, sealed (`ome-signer`, Argon2id)                                                               |
| `keeper-key.sealed`           | the keeper's master key, when no OS keychain holds it                                                          |
| `secrets/*.sealed`            | the member's Aztec account and each vault's protocol secrets, sealed with the master key (AES-256-GCM)         |
| `vaults/*.json`               | each vault's record: name, founding constructor (public), names, each epoch's transcript, the last config read |
| `rotations/*.json`            | rotations this daemon takes part in                                                                            |
| `keeper/`, `keeper.log`       | the keeper's data directory and log                                                                            |
| `pxe/`                        | the PXE's store                                                                                                |
| `daemon.addr`, `daemon.token` | written at start: the socket path and the session token (`ome-client` discovers them)                          |
| `daemon.lock`                 | one daemon per profile                                                                                         |

The socket is `daemon.sock` in the profile, unless that path is longer than 100 bytes; it then lives in the temporary
directory under a hash of the path.

## Networks

This release runs on the local network and on Testnet (`src/node/networks.ts`), recognised by the node's L1 chain and
rollup version. `profile.create` refuses any other node, and Aztec's mainnet (Alpha) with "not released for Aztec's
mainnet yet" until the audit; a profile whose node later moves to an unsupported network does not start its core. A
new Testnet profile gets Ome Labs' relay (`relay.ome.network`, peer id `12D3KooWEKh8…XXYWo`) unless relays were
already chosen; a local network needs none.

## The server

- Newline-delimited JSON-RPC 2.0, the keeper's framing.
- Every connection presents the token first (`session.auth`), compared in constant time.
- `events.subscribe` adds `event` notifications, numbered per daemon start, and replays the ones after `since` from
  the last 1,024 (`architecture/api.md`, Daemon events).
- Errors carry the API kind, code and details. Errors from the keeper, from `@ome-labs/wasm` and the wallet's
  `VaultTxError` keep their kinds and details, also when Aztec wraps them (the server looks down the `cause` chain);
  anything else is `failed`.
- All 60 daemon methods are served. The server answers `session.auth` and `events.subscribe` itself, and the other
  58 each have a handler (`src/server/handlers.ts`) that checks the params it uses. Hex values and points are checked
  again by the bindings.

## Signer and keys

- `profile.create` first checks the node (the account lives on it) and refuses with `not-ready` when it doesn't
  answer, so a profile is never left half made. Then it does three things:
  1. seals a new signing key under the passphrase;
  2. creates the keeper's master key;
  3. creates the member's own Aztec account (an initializerless Schnorr account), sealed with the master key.
- `signer.unlock` opens an `UnlockedSigner` for a window. The key stays in WebAssembly memory and is wiped when the
  window ends or on `signer.lock`.
- A proposal can also be signed with a secret passed for that one call.
- **Keeper master key**:
  - it lives in the OS keychain: macOS Keychain, or Linux Secret Service through `secret-tool`. The keeper then starts
    with the daemon, without the member;
  - without a keychain, or with `OME_KEYCHAIN=off`, it is a file sealed with the signer's sealing key. The keeper then
    starts at the first unlock, by passphrase or by sealing key. A start that fails (the node down) is not kept: the
    next unlock tries again.
- The node URL is read as the daemon starts: after `config.set` changes it, the daemon must restart (`ome` does).

## Keeper supervision

The daemon runs `ome-keeper run --secrets-stdin`. The master key and a fresh session token go on stdin, never on the
command line or in the environment.

- The daemon reads the ready line, connects, and refuses a keeper whose `apiVersion` differs.
- It restarts the keeper with backoff when it exits, and reports the state (`keeper` events).
- Keeper events are forwarded (`keeperEvent`).
- The binary is found at `OME_KEEPER_BIN`, then `<home>/bin/ome-keeper`, then the repository's debug build.
- **Remote keeper**: `keeper.connect { remote: { address, fingerprint, token } }` makes the daemon use a keeper on
  the member's own server instead of running one (`ome-keeper run --rpc-listen`, `architecture/keeper.md`). The
  daemon connects over TLS, uses the connection only if the certificate's SHA-256 fingerprint is the pinned one,
  then presents the token. The address and fingerprint are in `settings.remoteKeeper`; the token is sealed in
  `secrets/`. It reconnects with backoff when the connection drops; `keeper.connect { remote: null }` returns to the
  daemon's own keeper. Vaults stay with the keeper holding their shares, so a member switches before founding or
  joining them. A remote keeper's relays are its own configuration.
- Event listeners are subscribed again on every new keeper connection, local restart or remote reconnection.

## Fees

Every transaction pays through a `SponsoredFPC` instance, chosen by its salt (`settings.feePayerSalt`, changed live
with `config.set`):

- **zero** (the default): the canonical instance the local network and Testnet provide;
- **another salt**: a member's own deployment of `SponsoredFPC`, funded with Fee Juice, for a network where nobody
  sponsors (Alpha). Deploy it with that salt and bridge Fee Juice to its address; the daemon derives the address
  from the salt.

The vault never pays its own fees, and never in public: the fee payer contract does. The same address is the fee
payer the signer trusts in rotations and the one paired devices are told about.

## Founding

Lobbies have a size (`lobby.create { members }`); the host can remove a joiner (`lobby.remove`, which rotates the
invite) or change the size and thresholds (`lobby.update`) before starting. `lobby` events carry what changed. Each
member's Aztec account comes with the roster and is kept in the vault record beside the names (`accounts`); rotation
plans carry joiners' accounts from their join requests. Reviews and decoded calls name a member's account as
`<name> (member)`, ahead of `aztec-wallet` aliases.

**Names.** `vault.rename` and `profile.rename` go through the keeper's signed renames (`names.put`), so every member
sees them; the daemon applies them from `name` keeper events and on each sync (`names.list`), and emits `vaultRenamed`
and `memberRenamed`. A rename is metadata, not policy: it needs no proposal, any member can rename the vault, and only
a member can rename themselves. The history stays in each keeper.

**Passphrase.** `signer.changePassphrase { current, next }` re-seals the signer and, when a sealed file holds the
keeper's master key (no OS keychain), that file too, under the new sealing key. Both new files are written beside the
old ones (`.next`) before either replaces its original, and the daemon finishes an interrupted swap at start, so the
passphrase never opens one and not the other. It emits `sealingKeyChanged`; a client that kept the sealing key (Touch
ID) fetches it again.

**Deleting a profile.** `profile.delete { confirm }` refuses while the profile is in a vault that isn't `removed`
(its viewing share and signer would be lost); otherwise the daemon stops, deletes its keychain entry and directory, and
exits.

1. The host's `lobby.create` opens a lobby through the keeper. The invite carries the network label
   `aztec:<l1 chain id>:<rollup version>`, and `lobby.join` refuses an invite for another network.
2. `lobby.start` fixes the thresholds, a fresh salt and fresh protocol secrets. The keeper seals the secrets to each
   member.
3. Each member runs `founding.begin`. It goes through these steps, reported as `founding` events:

| Step     | What happens                                                                                                                                                                                                             |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| agree    | wait for the host's plan                                                                                                                                                                                                 |
| dkg      | `ceremony.dkg` with the roster in member order; every keeper must reach the same transcript                                                                                                                              |
| derive   | `deriveVault` from the roster's signer keys, the DKG's group key and verification shares, the plan's thresholds, salt and secrets                                                                                        |
| register | `vault.register` with the manifest; the vault's record and sealed secrets are stored; the wallet registers the vault                                                                                                     |
| deploy   | the host deploys from its own account, retrying for up to 2 minutes while a co-founder's keeper has not registered yet (deploying reads the vault's keys by threshold ECDH); the others wait until the config note reads |
| verify   | the config note goes to the keeper (`vault.observeConfig`), which must answer `current`                                                                                                                                  |

## Proposals

- **`proposals.create`**: turns an intent into an Aztec payload, with the fee payer's call merged in first: the fee
  payer ends Aztec's setup phase, and a public call enqueued before it would run as setup, which only allow-listed
  functions may.
  - A transfer is the token's private `transfer`.
  - `calls` are built from the contract's artifact: each `AbiValue` is checked against its parameter's type, then
    Aztec's encoder encodes it. The payload's extras list the classes the calls run and the instances the node
    doesn't publish (`PayloadContracts`), and the daemon offers those artifacts to its keeper.
  - A `calls` intent may grant `authwits`: `{ caller, call }`, at most `limits.maxAuthwits` (4). Each call is built
    the same way and must be private, and each caller must be one of the intent's own call targets; the wallet
    signs their hashes with the transaction (`architecture/protocol.md`). Views, previews and reviews decode each
    authwit from the payload (a call without a function name decodes by its selector), name its caller as the call
    it is, and show the hash and the message an approval signs for it. The daemon rebuilds nothing else for them:
    execution adds each approved authwit's witness through the wallet.
  - A cancellation is the fee call alone, under the cancelled proposal's nonce.
  - `setThreshold` and `invalidatePending` call the vault itself; a threshold outside 1 to the member count, or equal
    to the current one, is refused before anything is built. Once a threshold change runs, each member's keeper adopts
    the new threshold into its manifest (`vault.updateSigners`, as for a signer swap). Before building or rebuilding
    any proposal, and right after executing one, the daemon checks the config on chain against the manifest and
    observes it when the signers or threshold differ, so no member acts on a stale policy until the next sync.

  The wallet builds the proposal, and the daemon stores the payload and the proposal with the keeper, which
  replicates them. The proposal's summary is the intent as JSON.

- **Views**: built from the keeper's records.
  - Votes are verified against the signer set.
  - The status comes from the tally at chain time, the receipts, nonces spent by other executed proposals, and the
    config version.
  - Calls come from the checked payload, decoded with the contract's artifact when this member has it (named and typed
    arguments, labels for the vault, its tokens and aliases, token amounts in whole tokens).
- **`proposals.review`**: rebuilds the transaction on this device. `vaultTxOf` runs the wasm checks and Aztec's own
  outer hash. Review then simulates the transaction with the vault's class overridden by `OmeVaultPreview` (the vault's
  storage and functions, an entrypoint that needs no approvals; `pxe-integration.md`), so calls to the vault itself
  run the contract's own checks, and shows what an approval and a rejection would sign.
  - **Effects** (`src/proposals/effects.ts`): the wallet's `simulationFacts` reads the simulation without proving:
    the vault's notes it spends (its nullifiers matched against the vault's notes in the PXE) and the notes it creates
    for the vault (note hashes recomputed with the vault as owner). Notes of a tracked token's single-value shape give
    each token's balance after, from its balance before; anything else sets `complete: false`.
  - **Failures** (`src/proposals/failures.ts`): check failures and simulation errors are mapped to kinds a client acts
    on (`architecture/api.md`); a failure that matches no rule keeps its own message.
  - A threshold change carries the before, the after and the policy warnings of the new shape (`@ome-labs/wasm`).
- **`proposals.preview`** builds and simulates an intent the same way before it is proposed, so the proposer sees the
  same effects and failures; an intent that can't be built (a bad argument) is refused, not reported.
- **`approve`** / **`reject`**: sign with the signer, then store the vote with the keeper.
- **`execute`**: refuses with `conflict` while another member's claim is live (unless `force`), then claims the
  proposal with the keeper at each phase (rebuilding, proving, sending, then mined or failed), so every member sees
  who is executing and how far. Before proving, it checks the fee payer: a healthy sponsored payer is trusted, the
  member's own payer is checked against the transaction's simulated fee. It then tallies the votes into the
  witness, sends the transaction, and stores the receipt with its block; the keeper signs it as this member's.
- **Lifecycle events** (`src/proposals/events.ts`): from the keeper's events, `proposalExecuting` (claims),
  `proposalExecuted` with who executed it, and `proposalCancelled` when a cancellation runs; every sync interval
  (15 s), `proposalExpiring` an hour before an open proposal's deadline and `proposalExpired` after it, once each per
  daemon run.
- **Fees** (`src/fees.ts`): `fees.status` reads the fee payer's Fee Juice from the node; the daemon keeps the last read
  for `status` and refreshes it every sync interval and after each execution.

## Contracts

- **Registering** (`contracts.register`): the daemon computes the artifact's class id and refuses it unless the
  contract at the address runs that class: the instance the node publishes (its current or original class), or one
  derived from the given deployment, which must derive the address. The artifact is kept gzip-compressed in the
  profile's `contracts/` directory, registered again at every start, and offered to the keeper.
- **A proposal's contracts**: approving needs no artifact (members sign the payload's calls as they are), but decoding,
  simulating and executing do. When a proposal arrives, and before a review or an execution, the daemon makes its
  contracts usable: each class this PXE lacks is fetched through the keeper, recomputed and refused if it doesn't
  match, then registered; each target's instance comes from the node or from the payload's extras, checked against its
  address. A class still on its way is announced with `contractAvailable` once it arrives; until then the review's
  calls are marked `decoded: false`.
- **Limits** are in `src/limits.ts` and reported by `daemon.info`.

## Tokens and activity

- `tokens.add` registers a token's instance in the PXE: the one the node publishes, or, for a token whose instance
  was never published, the one its deployment (`deployment`: salt, deployer, and the constructor's admin, name,
  symbol and decimals) derives, refused if it derives another address. A token with its own code (not Aztec's
  standard `Token` class) is accepted when this member registered its artifact (`contracts.register`) and it has
  `balance_of_private`, `transfer`, and name, symbol and decimals getters (`private_get_…` or `public_get_…`);
  otherwise it is refused naming what is missing. `TokenInfo.standard` tells the two apart.
- `tokens.list` reads each token's **private** balance (the vault's notes, through the keepers) and **public**
  balance (`balance_of_public`, read from the member's own account, so it shows when the keepers can't answer), each
  as `known { amount }`, `unreadable { answered, needed }` or `failed { message }`. `unreadable` comes from the
  keeper's `Unavailable` error, found through the error's `cause` chain or, when Aztec wrapped it, its message
  (`only 1 of 2 members answered`). A balance already synced reads without the keepers; a new note can't.
- **Receiving.** Payments are found without registering their sender: a sending wallet's default tagging strategy
  is a non-interactive handshake through the standard HandshakeRegistry, and every contract's sync asks the
  registry for the vault's handshakes and derives their secrets through threshold ECDH (`pxe-integration.md`).
  `aztec-wallet`'s accounts are also registered as senders, for wallets that tag by address. `vault.receiving`
  reports both and the last payment; `vault.request` builds an `ome-pay:` link (`wire.md`).
- **Not found:** transfers in a contract this member's PXE doesn't know. Aztec's tags are siloed per contract and the
  PXE syncs a contract's notes only when it runs one of its functions, so a token must be tracked (or its contract
  registered and called) before its notes to the vault are found.
- `vault.activity` lists proposals and **received transfers**: the tracked tokens' private `Transfer` events to the
  vault that its PXE decrypted (through threshold ECDH, like notes), timed by their blocks. The vault's own change is
  not "received".
- `vault.counterparties` lists who paid the vault. A counterparty that paid before a member was removed is
  **exposed**: the removed member's PXE derived the tagging secret it shares with the vault, so it can still tell
  when that counterparty pays again, never what or how much, until the counterparty handshakes with the vault anew.
  When a rotation that removed members activates, the daemon records it (in chain time) and emits
  `handshakeAdvised` if any counterparty is exposed, so the member can ask them to.
- Ome keeps no address book: names for addresses come from `aztec-wallet`'s `accounts:` and `contracts:` aliases
  (reviews, counterparties, a phone's labels). Every `accounts:` alias is registered in the PXE as a tagging source
  (`registerSender`), at start and with each 15-second sync, so its notes to the vault are found by the tags they
  share with it.
- `tokens.remove` stops tracking a token.
- **Paging** (`src/paging.ts`): `proposals.list` (with `filter`: `all`, `open`, `ready`, `done`, `needsMe`),
  `vault.activity` (with `kinds` and `token`), `vault.audit` and `vault.counterparties` return `{ items, next }`,
  newest first, at most `limit` (50; 1 to 500). The cursor is the last item's time and id, and the next page holds
  strictly older items, so new arrivals never shift a page. `status.vaults[].counts` (open, ready, needs my vote) is
  recounted after proposal and vote events and every sync.

## Working with aztec-wallet

`aztec.aliases` reads `aztec-wallet`'s typed aliases (`accounts:…`, `contracts:…`, `artifacts:…`) from its own store
(`@aztec-labs/cli-wallet` keeps them in the `aliases` map of an LMDB store, `@aztec-labs/kv-store` 6.0.0) in
`settings.aztecWalletDir`, default `~/.aztec/wallet`. The store is opened for each call only. A directory without a
store is checked first: opening a missing one aborts the process from the native binding.

With `settings.aztecWalletSync` on, `vault.alias` also writes `accounts:<alias>` into that store, so `aztec-wallet` can
pay the vault by name; `aztec.alias` writes one alias. Sync is off by default.

## Membership

1. **Join request.** A newcomer's daemon produces one (`membership.request`), signed by its keeper.
2. **Plan.** `membership.plan` builds the plan string and checks:
   - that at most `t_v − 1` members leave;
   - that `t_v` members stay to deal; dealers are always members who stay, as the epoch tagging secret requires;
   - the thresholds against the new size;
   - that each joiner's keeper isn't already a member.
3. **Begin.** Everyone who stays and every joiner passes the plan to `membership.begin`. The plan carries the
   reshare's parameters, the current manifest and config, the vault's founding constructor, names, and the rotation
   proposal's nonce. It holds no secrets.
4. **Reshare.**
   - Dealers pass `ceremony.reshare` the hand-off: the protocol secrets and every earlier epoch's transcript, for at
     most 12 earlier epochs (`src/secrets/handoff.ts`).
   - Each dealer's keeper seals the hand-off to the joiners, so a member removed earlier never receives it.
   - The new epoch's transcript is stored.
5. **Propose.** The planner proposes `rotate_membership` under the plan's nonce, built from its own reshare result.
6. **Approve.** Members approve with `proposals.approve`, which routes a rotation to `approveRotation`, signing with
   the unlocked signer or the secret passed with the call, as for any proposal. The proposal
   must perform exactly the rotation this member's keeper reshared. That compares the signers and committee shares, so
   an approver never signs a rotation it did not take part in. The member's trusted fee payer is the only other call
   allowed.
7. **Execute.** Any member executes it.
8. **Activate.**
   - Members who stay read the new config note and hand it to the keeper, which activates its pending share. The
     daemon then points the wallet at the new committee and its epoch's tagging secret.
   - A joiner can't read the note until its keeper activates. It waits for the plan nonce's nullifier on chain
     (`poseidon2([nonce], 1025801951)`, siloed with the vault), computes the config the rotation sets
     (`checkRotation`), and hands that to its keeper.
   - If the proposal is cancelled instead, the nullifier also appears. That's harmless: the members who stay never
     activate a committee the chain doesn't name, so a joiner alone can't reach `t_v`.
9. **Retire.** A member who leaves without taking part can no longer read the vault: the new committee doesn't answer
   its keeper. It retires its share with `vault.retire`.

Replacing a device is the same flow: the old device's slot leaves and the new device joins.

## Paired devices

A member can move their signing key to a phone (`architecture/mobile.md`). The keeper pairs the phone and carries
its reviews and votes; the daemon decides what the phone is asked (`src/devices/service.ts`).

- **Pairing**: `devices.pair` asks the keeper for an `ome-pair-v0:` offer naming the member, the network label and
  the fee payer this daemon trusts. Pairing needs a relay (`relays.set`).
- **Enrolling**: `devices.enroll { vault, identity }` proposes the signer swap that puts the device's key in this
  member's slot (`membership.swapSigner`, below). `devices.list` shows, for each device, the vaults where it holds
  this member's key.
- **Forwarding**: on every `proposal` and `payload` keeper event, each not yet executed proposal of a vault where a
  paired device holds this member's key is sent to it with `devices.review`. The daemon adds what only it knows:
  the config it read, the chain, labels from `aztec-wallet`'s aliases and tracked tokens, its own review's simulation, and for a
  reshare rotation the rotation its keeper reshared. `devices.forward` sends one again.
- **Voting**: in such a vault this profile's key is no longer the member's, so `proposals.approve` and
  `proposals.reject` refuse with "approve or reject there". The device's vote reaches the keeper, which stores and
  replicates it like the daemon's.
- **Fast polling**: while an offer is open or a forwarded proposal awaits the device's vote, the daemon has the keeper
  poll its mailboxes every 3 seconds (`mailbox.poll`) for up to 10 minutes, instead of every 30.
- `devices.revoke` unpairs a device.

### Signer swaps

`membership.swapSigner { vault, slot, signerKey }` proposes `rotate_membership` with one slot's key replaced and the
committee, `k` and `t_v` unchanged: no reshare. It is how a member moves to a new device, or how other members
replace a lost one.

- Members approve it with `proposals.approve`: a rotation that `signerSwapSlot` recognises is signed with
  `approveSignerSwap` (one key replaced, nothing else, plus the trusted fee payer).
- After it executes, every daemon's next config read sees a signer set its manifest does not match. Its keeper then
  adopts the new key from the executed swap (`vault.updateSigners`), and the wallet is pointed at the updated
  manifest.
- The desktop's own key stays in the profile: it may still sign for the member's other vaults.

## Background work

Every 15 seconds, for each vault, the daemon reads the config note and hands it to the keeper:

- an activated committee re-points viewing;
- a signer set that no longer matches the manifest has the keeper adopt a landed signer swap;
- a removal marks the vault;
- anything else surfaces through the keeper's alarms.

## Tests

| Test                             | What it checks                                                                                         |
| -------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `src/server/server.test.ts`      | the socket: authentication, refused tokens, error kinds and codes, events, `0600`                      |
| `src/proposals/review.test.ts`   | a payload that rebuilds to another outer hash is reported, and approval refuses it                     |
| `src/proposals/failures.test.ts` | check and simulation failures map to their kinds, and unknown ones keep their message                  |
| `src/membership/plan.test.ts`    | plans the rules refuse report why, dealers are members who stay, and a member with no part can't begin |
| `test/e2e/daemons.test.ts`       | the local-network suite, below                                                                         |
| `test/e2e/device.test.ts`        | a member's phone on the local network with a real relay, below                                         |
| `test/e2e/remote-keeper.test.ts` | a member's keeper on their own server, reached over TLS, below                                         |

`test/e2e/daemons.test.ts` runs against `aztec start --local-network`. Three, then four, daemons run in one process,
each with its own keeper process and PXE, all driven through their sockets:

- they found a vault and agree on its address;
- they pay with two approvals, and a rejection blocks a proposal;
- a newcomer joins and a member leaves in one rotation;
- the newcomer reads the vault and its approval counts;
- the member who left can't see the new balance and retires its share.

Measured: 4 tests in 79 s with fake proofs, setup included.

`test/e2e/device.test.ts` adds a real `ome-relay` on loopback and the `ome-device` test phone:

- three daemons found a vault, and Alice's daemon pairs the phone through the relay;
- a signer swap moves Alice's key to the phone, and all three keepers adopt it;
- Alice's daemon forwards a payment to the phone, which finds it signable; Alice's desktop key is refused; the
  phone's approval reaches Bob, and the payment executes; the phone learns its vote was accepted;
- revoking the phone ends its pairing.

Measured: 5 tests in 43 s; the whole suite, 9 tests in 113 s.

`test/e2e/remote-keeper.test.ts` runs a keeper as on a server (passphrase, `--rpc-listen` on loopback):

- Bob's daemon refuses it under a wrong fingerprint, then uses it;
- Alice (her own keeper) and Bob (the remote one) found a vault, pay from it, and Bob reads its balance through
  the remote keeper.

`test/removal/removal.test.ts`, the removal drill, removes Carol from a 2-of-3 vault with `t_v = 2`, then from a
3-of-4 vault with `t_v = 3`. Her keeper's whole state is copied before the removal.

- The members who stay read a payment sent afterwards.
- No verification share of the new committee is an old one, and the new committee's partials do not verify as
  partials of the old committee, so a share of the old one cannot complete a combination with them.
- Her keeper, restarted from the copy, gets no answer for its old epoch.
- Her approval of a payment left open goes stale and the payment cannot execute; the vault keeps its address.

Measured: 10 tests in 157 s.
