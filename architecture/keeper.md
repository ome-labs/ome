# ome-keeper

`crates/keeper` is a member's **always-on service**. It holds the member's viewing share for each vault and never
lets it out. It:

- runs the DKG and reshare ceremonies with the other members' keepers;
- answers threshold ECDH requests with DLEQ-proven partials;
- gathers partials for its own member;
- activates, holds or destroys shares as the vault's on-chain committee changes.

It is a library first. `Keeper::start` takes its transport (`ome-net`), clock and storage from outside, so tests run
many keepers in one process and an app can embed one.

**Status:** checkpoint 7a is in the tree: custody, store, ceremonies, threshold ECDH and the custody rules below.
Checkpoint 7b adds proposals and replication, sync, mailboxes, lobbies, the local RPC server and the binary.

## What the keeper never does

- It never sees a signing key. Approvals are signed by `ome-signer` on the member's device.
- It never talks to Aztec. The member's daemon reads the chain and reports what it observed (`observe_config`). The
  keeper checks those reports against its own records before acting on them (below).
- It never sends its viewing share anywhere, except as reshare sub-shares sealed to the new committee.
- It never reconstructs the viewing key; it only produces partials.

## Custody

- **Master key**: 32 bytes that seal every secret at rest. It comes from the desktop app (OS keychain) or, for a
  keeper on a member's own server, from a passphrase via Argon2id (64 MiB, 3 passes) with a per-keeper salt.
  A sealed check value detects a wrong key before any secret is touched.
- **Sealed files**: one file per secret under `<data>/secrets/`.
  - Each is sealed with XChaCha20-Poly1305 under the master key, with associated data `"ome.keeper.sealed.v0:" ‖
    name`. A file moved under another name doesn't open.
  - Writes are atomic: temp file, fsync, rename, then fsync of the directory.
  - Files are mode `0600` and directories `0700`.
  - `destroy` overwrites a file before unlinking it.
  - `rename` re-seals under the new name.
- **Keys**: the keeper's Ed25519 identity (also its libp2p peer id) and its X25519 transport key, created on first run.
- **Viewing shares**, by slot:

  | Slot                    | Holds                                                |
  | ----------------------- | ---------------------------------------------------- |
  | `dkg-<session>`         | a finished DKG's share, waiting for `register_vault` |
  | `pending-<session>`     | a finished reshare's share, waiting for its rotation |
  | `share-<vault>-<epoch>` | the share serving the vault at that committee epoch  |

  All share storage goes through one interface (`StoredShare`), so a recovery design (encrypted backups or a recovery
  keeper) can be added later without restructuring.

## Store

SQLite in WAL mode with `secure_delete` on. It holds only public, non-secret state:

- manifests by vault and epoch, with their status (`active`, `retired`, `removed`);
- ceremony records with their parameters and results;
- the ECDH audit log;
- every config observation in full.

Migrations are append-only and tracked by `user_version`.

## Ceremonies

DKG and reshare are state machines over signed messages and clock ticks. They emit messages to send and a status.
There is no coordinator: members are indexed `1..=n` in the agreed order.

- **DKG**:
  1. Round 1 broadcasts commitments and a proof of knowledge, and every participant **echoes** each dealer's signed
     broadcast. Two different signed versions prove equivocation.
  2. Round 2 sends each member its share in an HPKE envelope.
  3. A member whose share doesn't verify files a **complaint**. The accused reveals the share (**justification**), and
     the dealer is blamed if it's bad. If it's good, the ceremony restarts without blame, since the share is now
     public.
  4. Every member confirms the same transcript.
- **Reshare**:
  1. Exactly `t_v` current members deal sub-shares of their weighted shares.
  2. New members check every deal against the current verification shares, combine their sub-shares and confirm the
     transcript.
  3. Every participant computes the new committee's verification shares from the dealers' commitments, so a dealer
     that leaves the committee also knows the commitment its rotation will put on chain.
  4. The result (`ReshareResult`) records that commitment, and for new members the new manifest. Their new share is
     stored as `pending`.
- **Timeouts**: a phase past its deadline aborts and blames whoever didn't send what it needed.
- **No resume**: ceremony secrets live only in memory. A keeper that restarts mid-ceremony marks the ceremony aborted,
  and members start a new session.

## Threshold ECDH

- **Serving**: a request from another member is served only if:
  - the vault is held and active;
  - the epoch is the current one;
  - the requester is in the current manifest;
  - its token-bucket budget (points per minute) allows it.

  Each answer is one partial per point with a DLEQ proof. Every request, served or refused, is written to the audit
  log with the requester, purpose, point count and outcome.

- **Gathering** (`ecdh_batch`, for the member's daemon): the keeper computes its own partials, asks the other members,
  verifies every partial's proof against that member's public address share, and returns `t_v` members' partials.
  - A member whose partials fail verification is reported (`InvalidPartials`) and routed around.
  - Fewer than `t_v` valid answers before the timeout gives `Unavailable`.

## Acting on the chain

The daemon reports each config it observes with `observe_config(vault, config)`, where `config` is the full config
note. The keeper acts only on a config it can verify:

| Observed config                                                                                                                                           | Outcome     | What happens                                                       |
| --------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------ |
| the committee it already serves                                                                                                                           | `Current`   | nothing                                                            |
| a version not newer than the serving committee                                                                                                            | `Outdated`  | nothing                                                            |
| the committee of one of its own completed reshares, and the new manifest passes `ome_protocol::membership::Vault::from_manifest` against the whole config | `Activated` | the pending share becomes active, then the old share is destroyed  |
| the committee of its own completed reshare that leaves this keeper out                                                                                    | `Removed`   | its share is destroyed                                             |
| anything else                                                                                                                                             | `Unknown`   | **the share is kept**, nothing changes, `ConfigMismatch` is raised |

This closes the gap where any unrecognised newer config made a keeper delete its share. A buggy or malicious rotation
can no longer make every keeper destroy the vault's ability to read. Holding a share that is no longer needed is
recoverable; destroying one that is still needed is not.

A member removed without taking part in the reshare (neither dealer nor new member) sees an `Unknown` config and
keeps its share. The removal still holds cryptographically, because the old shares no longer combine with the new
committee. Erasing that leftover share is a deliberate step, not an automatic one; the RPC for it comes in 7b.

**Pending shares expire.** A completed reshare whose committee never becomes active is expired after
`pending_share_ttl_secs` (default 7 days): its pending share is destroyed, the ceremony is marked `expired`, and
`PendingExpired` is raised.

**Later hardening:** the keeper trusts its daemon to _report_ configs; it doesn't trust the daemon about _what they
mean_. Reading the config note directly would remove the first trust as well. That needs PXE-style private note
reading in the keeper, so it is deferred.

## Events

These are streamed to the daemon:

- `Ceremony` (running, completed, aborted, activated, expired);
- `InvalidPartials` (vault and member);
- `CommitteeActivated` (vault and epoch);
- `Removed`;
- `ConfigMismatch` (vault, version and reason);
- `PendingExpired`.

## Tests

- `tests/custody.rs`:
  - secrets round-trip, never appear in plaintext, and are owner-only;
  - a wrong master key is caught;
  - tampered or swapped files are refused;
  - destroy and rename behave;
  - keys and shares survive a reopen;
  - passphrase keys depend on the salt.
- `tests/ceremonies.rs` drives the state machines in memory:
  - DKG agreement and ECDH through any qualified set;
  - a bad share the dealer stands by blames the dealer;
  - a valid justification blames nobody;
  - an equivocating dealer never yields a key;
  - a silent member is blamed;
  - reshare keeps the secret;
  - a dealer dealing another value is blamed.
- `tests/ecdh.rs` runs three keepers on the in-memory network:
  - every member gathers the same secret, and answers are audited;
  - other epochs and unknown vaults are refused;
  - a lying member is reported and routed around;
  - peers enforce the budget.
- `tests/rotation_custody.rs`:
  - a verified rotation activates the new share and removes the leaver, with old share files gone;
  - an unverifiable config keeps the share and alarms;
  - a reshare committee with a mismatching config isn't activated;
  - an unlanded pending share expires;
  - a restart keeps the vault and aborts an interrupted ceremony;
  - a silent member times a ceremony out and is blamed.

Tests use a manual clock and wait on the keepers' own events, with a time limit, instead of sleeping.
