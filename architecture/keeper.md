# ome-keeper

`crates/keeper` is a member's **always-on service**. It holds the member's viewing share for each vault and never
lets it out. It:

- runs the DKG and reshare ceremonies with the other members' keepers;
- answers threshold ECDH requests with DLEQ-proven partials;
- gathers partials for its own member;
- activates, holds or destroys shares as the vault's on-chain committee changes;
- stores and replicates proposals, payloads, votes and receipts between members;
- hosts and joins founding lobbies;
- uses relay mailboxes so offline members catch up;
- serves its member's daemon over a local JSON-RPC socket.

It is a library first. `Keeper::start` takes its transport (`ome-net`), clock and storage from outside, so tests run
many keepers in one process and an app can embed one.

It also ships as the `ome-keeper` binary (below).

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

  A joining member's keeper also keeps the protocol secrets a reshare handed it, under `joiner-secrets-<session>`.

  All share storage goes through one interface (`StoredShare`), so a recovery design (encrypted backups or a recovery
  keeper) can be added later without restructuring.

## Store

SQLite in WAL mode with `secure_delete` on. It holds only public, non-secret state:

- manifests by vault and epoch, with their status (`active`, `retired`, `removed`);
- ceremony records with their parameters and results;
- the ECDH audit log;
- every config observation in full;
- proposals, payloads by hash, approvals, rejections and receipts;
- contract artifacts by class, in chunks, with their manifest once known;
- lobbies: the public roster and plan. The invite secret lives in custody.

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
  5. **Joiner secrets.** A member who joins after founding also needs the vault's shared protocol secrets
     (nullifier-hiding, outgoing-viewing and tagging keys).
     - A dealer's daemon passes them in `ceremony.reshare`.
     - The dealer's keeper seals them to each joiner (a member of the new committee but not the old) with the
       lobby's HPKE sealing, bound to the session and the joiner's new index, and sends them as `ReshareSecrets`.
       It never stores them.
     - The joiner's keeper opens a hand-off only from a dealer of the session, and keeps it sealed at rest
       (`joiner-secrets-<session>`). `ceremony.status` returns it to its daemon in the result's `secrets`.
- **Late starts**: members' daemons start a ceremony when their members act, so frames often arrive before the session
  exists. Every ceremony frame for a session this keeper hasn't started waits in a bounded backlog (16 sessions, 64
  frames each) and is replayed when the session starts. This holds whoever signed it, including fellow committee
  members, whose delivered frames are never resent.
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
committee. Erasing that leftover share is a deliberate step, not an automatic one: `vault.retire` destroys it, and
only once the chain has moved past the committee it serves, so a mistaken call can't break a working vault.

`vault.retire { force: true }` skips that check. Such a member can't read the new config to show its keeper, since
the config is encrypted to the new committee. A member may always destroy its own share; doing so costs only its own
access.

**Pending shares expire.** A completed reshare whose committee never becomes active is expired after
`pending_share_ttl_secs` (default 7 days): its pending share is destroyed, the ceremony is marked `expired`, and
`PendingExpired` is raised.

**Later hardening:** the keeper trusts its daemon to _report_ configs; it doesn't trust the daemon about _what they
mean_. Reading the config note directly would remove the first trust as well. That needs PXE-style private note
reading in the keeper, so it is deferred.

## Spending records

- **Proposals and votes**: the daemon puts proposals, approvals, rejections and receipts, and the keeper stores them
  and pushes them to every member. A member that can't be reached gets them through its relay mailbox and the next
  sync.
- **Vote checks**: an approval or rejection is stored only if it is the Schnorr signature of the key in its slot over
  `ome-protocol`'s approval message (`vault`, `outer_hash`, config version, deadline) or rejection message.
- **Early votes**: a vote that arrives before its proposal is held, up to 1,024 votes, and checked when the proposal
  arrives. Records are processed concurrently, so this ordering is normal.
- **Payloads** are stored by hash and **never pushed**. A keeper that receives a proposal whose payload it lacks asks
  whoever sent the proposal (`PayloadRequest`), and only payloads that a known proposal of that vault references are
  accepted. A payload therefore never reaches a member that doesn't know why it exists, and the proposer needn't be
  online.
- **Unexpected spends**: `vault.observeTx` raises `UnexpectedSpend` for a vault transaction that matches no receipt.
- **Artifacts** are stored by contract class and, like payloads, **never pushed**. The daemon puts the artifacts its
  proposals call (`artifacts.put`, one gzip chunk per call; the manifest is the chunks' hashes once the last one is
  in). When a member's daemon needs one (`artifacts.fetch { vault, classId }`), its keeper asks the vault's other
  members for the manifest, adopts the first one that answers, and asks that member for the chunks, 16 at a time,
  keeping each only if it matches the manifest's hash. The `artifact` event reports the end: complete, or given up
  with a reason. A fetch that heard nothing for 60 seconds may start again. The store refuses new artifacts past
  512 MiB. Whether the artifact is the code of its class is the daemon's check.

Replicated records skip the replay window: they are idempotent, verified on their own, and may arrive days late
through a mailbox. Everything else must be fresh and unseen.

## Catching up

- **Sync**: every `sync_interval_secs` (default 60), the keeper sends each member a `SyncRequest` listing what it
  holds. Members answer with up to 1,024 records it lacks, proposals first, and its payloads then follow by hash.
- **Mailboxes**: a frame a member couldn't receive directly is sealed into that pair's daily mailbox
  (`ome_crypto::sealing::mailbox`) at the relays, for `mailbox_ttl_days` (default 7).
  - Every `mailbox_interval_secs` (default 30) the keeper polls the mailboxes of every committee and open-lobby peer
    for each day in the window.
  - It opens the envelopes and feeds them in as if they had arrived live, on a `Channel::Key`.
  - The sealed sender must be the member the frame claims.
  - Delivered envelopes are deleted at the relays.

## Lobbies

- **Host**: `open_lobby` creates an invite (`ome-invite-v0:`) holding a join secret, kept in custody. While the lobby
  is open, the transport admits strangers on the lobby lane only.
  - Each `LobbyJoin` must pass `ome_protocol::lobby::verify_join`: the joiner is the signer, and the proof matches
    the secret and the joiner's keeper keys. Duplicate signing or transport keys, or a full roster, are refused.
  - Every change sends the roster (`LobbyState`) to the guests.
- **Start**: `start_lobby` fixes the plan:
  - a fresh DKG session;
  - the thresholds and salt;
  - the members in slot order;
  - `protocol_version` from `plan_version`, the newest version every founder's keeper speaks;
  - founding secrets sealed to each member.

  It sends the plan to the guests and closes admission.

- **Guest**: joins with the invite, follows newer rosters from the host only, and accepts a plan only if:
  - it comes from the host;
  - it names this member exactly as it joined;
  - its thresholds and members are valid;
  - its version is one every member speaks;
  - this member's secrets open.
- **No automatic DKG**: a keeper never starts the DKG itself. Each member's daemon reads the plan and calls
  `ceremony.dkg`, so a person has agreed to found the vault.
- **Leaving**: guests can leave, and hosts can close.

## Paired devices

A member's signing device (a phone) pairs with the member's keeper, which then carries the device's reviews and
votes. The flows and the trust boundary are in `architecture/mobile.md`.

- **Offers** (`devices.offer`): a one-time 32-byte secret, kept in memory only, open for 10 minutes; at most 8 open.
  The offer (`ome-pair-v0:`) names the keeper, its configured relays, the member's name, the network label and the
  fee payers the daemon trusts. Pairing needs a relay.
- **Pairing**: every mailbox poll also fetches each open offer's pairing mailbox
  (`ome_crypto::sealing::pairing`). The first `DevicePair` that opens under the offer's secret, is signed by its
  sender and names the offer consumes it. The device's identity, transport key, signer key and name are stored in
  the `devices` table, and the keeper answers `DevicePaired` through their pairwise mailbox.
- **Reviews** (`devices.review`): the daemon names a stored proposal and passes what only it knows (the config it
  read, the chain, labels, its simulation and, for a reshare rotation, the rotation this keeper reshared). The keeper
  adds the manifest, the proposal and its payload and sends a `DeviceReview` to every device whose signer key is this
  member's key in the vault.
- **Votes**: a `DeviceVote` counts only if it arrives on a paired device's mailbox channel, is signed by that
  device, and its slot's key in the manifest is the device's signer key. It is then verified, stored and replicated
  exactly like a vote from the daemon, and the device gets `DeviceVoted` with the reason if it was refused.
- **Revoking** (`devices.revoke`) forgets the device and sends it `DeviceUnpair`; a device may unpair itself the same
  way. A revoked device's frames are dropped.
- Device frames skip the replay window, like replicated records: they are idempotent, verified on their own, and may
  wait days in a mailbox.

**Signer swaps.** Keepers check votes against the manifest's signer keys, so after a swap each keeper adopts the new
key with `vault.updateSigners(vault, proposal)`: the executed proposal's own `rotate_membership` call, through
`ome_protocol::membership::swapped_manifest`, must match the newest config the daemon observed. The committee, its
epoch and the share stay.

## RPC

The keeper serves the keeper API of `ome-api` (`architecture/api.md`):

- JSON-RPC 2.0, newline-delimited, over a Unix socket (`0600`), or a local-only named pipe on Windows;
- also over **TLS** with `--rpc-listen <host:port>` (or `rpc_listen` in `keeper.toml`), for a daemon on another
  machine. TLS 1.3 only, with a self-signed certificate made once and sealed in custody (`rpc-tls`), so its SHA-256
  fingerprint is stable; there is no certificate authority, the daemon pins the fingerprint. The ready line and
  `keeper.json` carry `rpcAddress` and `rpcFingerprint`. Sessions still present the token first;
- `session.auth` with the launch token first, compared in constant time;
- typed params and results;
- errors carry a kind (`not-found`, `invalid`, `not-ready`, `failed`) and its code.

`src/rpc/methods.rs` has one typed route per method, and `src/rpc/convert.rs` converts between the keeper's own types
and the API shapes. Points are checked to be on the curve on the way in. Events are `ome-api`'s `KeeperEvent`.

| Area      | Methods                                                                                                                                        |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| keeper    | `keeper.info`, `peers.status`, `audit.list`, `events.subscribe`                                                                                |
| vaults    | `vault.register`, `vault.observeConfig` (the whole config note), `vault.observeTx`, `vault.status`, `vault.list`, `vault.sync`, `vault.retire` |
| ceremony  | `ceremony.dkg`, `ceremony.reshare`, `ceremony.status`                                                                                          |
| ECDH      | `ecdh.batch`                                                                                                                                   |
| spending  | `proposals.put` / `get` / `list`, `payloads.put` / `get`, `approvals.put` / `list`, `rejections.put` / `list`, `receipts.put` / `list`         |
| lobbies   | `lobby.open`, `lobby.join`, `lobby.status`, `lobby.inspect`, `lobby.start`, `lobby.leave`, `lobby.close`, `lobby.list`                         |
| mailboxes | `mailbox.poll`                                                                                                                                 |
| devices   | `devices.offer`, `devices.list`, `devices.revoke`, `devices.review`, `vault.updateSigners`                                                     |

## Binary

`ome-keeper` has four commands: `init`, `identity`, `run` and `rpc`.

- **Master key**: the supervising app runs `ome-keeper run --secrets-stdin` and writes one JSON line holding the
  master key (kept in the OS keychain) and a fresh RPC token. A keeper on a member's own server derives it from a
  passphrase (Argon2id with a per-keeper salt), read from the file `OME_KEEPER_PASSPHRASE_FILE` names (a Docker or
  systemd secret) or from `OME_KEEPER_PASSPHRASE`, and writes its RPC token to `rpc.token`.
- **Config**: an optional `<data>/keeper.toml`, overridden by flags, sets:
  - listen, external and relay addresses;
  - the RPC socket;
  - timeouts and the ECDH budget;
  - the sync and mailbox intervals and the mailbox TTL;
  - the pending-share lifetime.
- **Startup**: `run` starts `Libp2pTransport`, plus `RelayMailbox` when relays are configured; the relay addresses
  also go into device pairing offers. When ready, it prints
  one JSON line with its identity, transport key, peer id, addresses and socket, and writes the same to
  `keeper.json`. Logs go to stderr as text or JSON.
- **Shutdown**: on SIGTERM or Ctrl-C it stops the node and removes the socket.
- **Data directory**: owner-only (`0700`). Secrets, the salt, the token and `keeper.json` are `0600`.

## Events

These are streamed to the daemon:

- `Ceremony` (running, completed, aborted, activated, expired);
- `InvalidPartials` (vault and member);
- `CommitteeActivated` (vault and epoch);
- `Removed`;
- `ConfigMismatch` (vault, version and reason);
- `PendingExpired`;
- `Proposal`, `Payload`, `Approval`, `Rejection` and `Receipt` as records arrive;
- `UnexpectedSpend`;
- `Lobby` (status and roster size);
- `Retired`;
- `DevicePaired`, `DeviceVote` and `DeviceUnpaired`.

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
  - a silent member times a ceremony out and is blamed;
  - a joining member receives the protocol secrets from the dealers, a staying member is sent none, and a non-dealer
    can't send them; the member who starts last still completes, and a hand-off that lands after completion is kept;
  - a member can force the retirement of its own share, which is otherwise refused while the committee is current.

Tests use a manual clock and wait on the keepers' own events, with a time limit, instead of sleeping.

Checkpoint 7b adds:

- `tests/proposals.rs`:
  - a proposal, payload and votes reach every member;
  - a payload is fetched from whichever member sent the proposal, even with the proposer offline;
  - a vote that arrives first is held until its proposal does;
  - forged or misdirected votes, and payloads of unknown vaults, are refused;
  - receipts replicate, and an unknown spend alarms.
- `tests/catch_up.rs`:
  - an offline member catches up through sync and through its relay mailbox;
  - an equivocating dealer is caught by the echo.
- `tests/lobby.rs`:
  - guests see the roster and receive the same plan, secrets and protocol version;
  - a join with the wrong invite secret is refused;
  - a guest leaves and the host closes.
- `tests/devices.rs`, a test phone on the in-memory mailbox:
  - an offer pairs one device once, and a request sealed under another secret is ignored;
  - the device receives a review with the manifest and proposal;
  - its vote for a slot whose key it does not hold is refused, and its own slot's vote is replicated;
  - a revoked device is told, gets no reviews and its votes are dropped;
  - a keeper adopts a swapped signer key only once the observed config holds it, keeping its epoch and share.
- `tests/rpc.rs`, on the wire over a real socket:
  - sessions must authenticate, and the socket is owner-only;
  - a daemon drives a live vault: ECDH, config observation, payloads, proposals and the audit log;
  - keeper errors, malformed params and unknown methods come back with their kind and code.
- `tests/rpc_tls.rs`: a client pinning the certificate's fingerprint authenticates and calls methods over TLS; one
  pinning another fingerprint never connects; the certificate survives a reopen of custody.
