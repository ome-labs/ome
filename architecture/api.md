# ome-api and ome-client

A member's device runs two local services. The **keeper** holds the viewing share; the **daemon** (`omed`)
hosts the PXE and the sealed signer and drives the keeper. Their APIs are defined once, in `crates/api` (`ome-api`):

- the daemon uses the keeper API;
- the CLI and apps use the daemon API.

`@ome-labs/api` carries the same types as TypeScript, and `crates/client` (`ome-client`) is the typed Rust client.
No API type is written by hand anywhere else.

The `core` feature converts the shapes to and from the core types (points, fields, manifests, configs), checking
points and fields on the way in. The keeper RPC and `ome-wasm` both use it.

## Conventions

- **Transport**: JSON-RPC 2.0, one JSON object per line. The connection is an owner-only Unix socket (`0600`), or a
  named pipe that refuses remote clients on Windows.
- **Sessions**: the first call of every connection is `session.auth { token }`. The token is the service's
  per-launch secret, compared in constant time. Until it succeeds, every other call fails with `unauthorized`.
- **Events**: after `events.subscribe`, the service also writes notifications `{ "method": "event", "params": E }`.
  The keeper's `E` is a `KeeperEvent`; the daemon's is an `EventEnvelope { seq, at, event: DaemonEvent }` (see
  [Daemon events](#daemon-events)). Events are tagged by `type`.
- **Shapes**:
  - field names are camelCase;
  - bytes are `0x`-prefixed lowercase hex: `Hex32`, `Hex64` (transcripts), `Hex16` (lobby ids), and `HexBytes` (the
    canonical `ome-wire` encoding of a proposal, payload or vote);
  - points are `{ x, y }`;
  - token amounts are decimal strings of base units (`Amount`, u128), because JSON numbers lose precision past 2^53;
  - times are Unix seconds;
  - a missing value in a result is `null`, and a field is never left out;
  - an optional param may be left out;
  - a method that takes nothing accepts `{}`, `null` or no params;
  - a method with nothing to return answers `null`.
- **Methods**: each is a marker type implementing `Method` (`NAME`, `Params`, `Result`), plus `KeeperMethod` or
  `DaemonMethod`. A client can therefore only send a method to the service that serves it.

## Errors

Every error is `{ code, message, data: { kind, details? } }`. Messages are for people and never contain a secret.
Clients act on the `kind`, and on `details` when the kind has them: the numbers needed to fix it, tagged with the same
`kind` so TypeScript narrows on it. Details a client doesn't understand read as absent; a kind it doesn't know reads
as `failed`.

**Rule:** a failure the member can act on gets its own kind, with details; `failed` is for bugs and unknown errors
only. `invalid` means the request itself was wrong.

| Kind                 | Code     | Meaning                                                                          |
| -------------------- | -------- | -------------------------------------------------------------------------------- |
| `parse`              | `-32700` | the line was not JSON                                                            |
| `invalid-request`    | `-32600` | not a JSON-RPC 2.0 request                                                       |
| `method-not-found`   | `-32601` | no such method                                                                   |
| `invalid`            | `-32602` | params, or a value inside them, refused                                          |
| `failed`             | `-32000` | anything else                                                                    |
| `unauthorized`       | `-32001` | no valid `session.auth` on this connection                                       |
| `not-ready`          | `-32010` | not yet: no profile, node unreachable, too few members answered, rate limited    |
| `locked`             | `-32011` | the signer is locked; unlock it or pass a secret                                 |
| `auth`               | `-32012` | the passphrase or key does not unseal the signer                                 |
| `alarm`              | `-32013` | the service refuses until a person looks (unverifiable config, unexpected spend) |
| `not-found`          | `-32014` | the vault, proposal, lobby or ceremony is unknown                                |
| `insufficient-funds` | `-32015` | the vault or the fee payer holds less than the transaction needs                 |
| `expired`            | `-32016` | the proposal's deadline has passed                                               |
| `stale`              | `-32017` | the proposal was made under a config the vault has since changed                 |
| `conflict`           | `-32018` | another member is already doing it (executing the same proposal)                 |
| `full`               | `-32019` | the lobby has as many members as it takes                                        |
| `unreadable`         | `-32020` | too few keepers answered to read the vault's private state                       |
| `unknown-contract`   | `-32021` | the contract is not registered with this member                                  |
| `network`            | `-32022` | the invite, token or vault belongs to another network                            |

Details per kind (`ErrorDetails`):

| Kind                 | Details                                                                                                | Raised by                                              |
| -------------------- | ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------ |
| `not-ready`          | `{ reason }`: `starting`, `profile`, `signer`, `node`, `keeper`, `sync`, `vault`, `payload`, `reshare` | the daemon wherever it waits for something             |
| `insufficient-funds` | `{ funds: { type: "token", token } \| { type: "feeJuice", payer }, needed, held }`                     | `proposals.create`, `proposals.execute`                |
| `expired`            | `{ at }`                                                                                               | `approve`, `reject` (the protocol), `execute`          |
| `stale`              | `{ made, current }` (config versions)                                                                  | `approve`, `reject` (the protocol), `execute`          |
| `conflict`           | `{ member, phase, since }`                                                                             | `execute`, once execution claims land (phase 21)       |
| `full`               | `{ members }`                                                                                          | the keeper's lobby join                                |
| `unreadable`         | `{ answered, needed }`                                                                                 | the keeper's threshold ECDH, and what reads through it |
| `unknown-contract`   | `{ address, classId? }`                                                                                | calls to a contract the PXE doesn't know               |
| `network`            | `{ expected, actual }`                                                                                 | `lobby.join`, `tokens.deployTest`                      |

`insufficient-funds` is checked against the vault's balance of the token when it can be read; a balance that can't be
read now (or a token the member doesn't track) is no reason to refuse, and the simulation decides.

The keeper maps its errors to kinds as follows:

- unknown vault or ceremony → `not-found`;
- bad input, wrong epoch, non-member, removed keeper, an existing ceremony, and cryptographic, wire and protocol
  refusals → `invalid`;
- too few members answering a threshold ECDH → `unreadable`, with `{ answered, needed }`;
- a lobby already at the most members a vault can have → `full`;
- rate limits and transport timeouts → `not-ready`;
- storage, custody, network and I/O failures → `failed`.

## Keeper methods

| Area      | Method                                                                       | Params → result                                                                |
| --------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| session   | `session.auth`, `events.subscribe`                                           | `{ token }` → `null`; `{}` → `null`                                            |
| keeper    | `keeper.info`, `keeper.joinRequest` (a signed `ome-join-v0:` string)         | → `KeeperInfo { apiVersion, version, identity, transport, peerId, addresses }` |
| vaults    | `vault.list`, `vault.status`, `vault.register`, `vault.retire`               | → `Vault { manifest, status, index, serving }`                                 |
|           | `vault.observeConfig`                                                        | `{ vault, config: VaultConfig }` → `{ observation }`                           |
|           | `vault.observeTx`                                                            | `{ vault, txHash }` → `{ known }` (an unknown spend raises `unexpectedSpend`)  |
|           | `vault.sync`, `peers.status`                                                 | `{ vault }` → `null`; `Reachability[]`                                         |
|           | `vault.updateSigners`                                                        | `{ vault, proposal }` (an executed signer swap) → `Vault`                      |
| ECDH      | `ecdh.batch`                                                                 | `{ vault, epoch, purpose, points }` → `{ epoch, responses: MemberPartials[] }` |
| ceremony  | `ceremony.dkg`, `ceremony.reshare`, `ceremony.status`                        | `DkgParams`, `ReshareParams` → `null`; `{ session }` → `Ceremony`              |
| spending  | `proposals.put` / `get` / `list`, `payloads.put` / `get`                     | `{ vault, body }` → `{ id }` or `{ hash }`; bodies as `HexBytes`               |
|           | `approvals.put` / `list`, `rejections.put` / `list`, `receipts.put` / `list` | votes as `HexBytes`; `Receipt { proposal, txHash, at }`                        |
| audit     | `audit.list`                                                                 | `{ vault, limit? }` (100 by default, at most 10,000) → `AuditEntry[]`          |
| lobbies   | `lobby.open`, `lobby.join`, `lobby.inspect`                                  | → `{ lobby, invite }`, `{ lobby }`, `Invite`                                   |
|           | `lobby.status`, `lobby.list`, `lobby.start`, `lobby.close`, `lobby.leave`    | → `Lobby` (with its `plan` once started)                                       |
| mailboxes | `mailbox.poll`                                                               | → `{ delivered }`                                                              |
| devices   | `devices.offer`                                                              | `{ member, network, feePayers }` → `{ offer, expiresAt }` (`ome-pair-v0:`)     |
|           | `devices.list`, `devices.revoke`                                             | → `Device[]`; `{ identity }` → `null`                                          |
|           | `devices.review`                                                             | `DeviceReviewParams` → `{ devices }`                                           |

`KeeperEvent` types:

- `ceremony` (session, kind, status);
- `invalidPartials`;
- `committeeActivated`, `removed` and `retired`;
- `configMismatch` and `unexpectedSpend`, the alarms;
- `proposal`, `payload`, `approval`, `rejection` and `receipt`;
- `lobby`;
- `pendingExpired`;
- `devicePaired`, `deviceVote` (with `refused` when the keeper turned it down) and `deviceUnpaired`.

## Daemon methods

Times are Unix seconds, and hex values are typed.

| Area       | Methods                                                                                                                                           |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| session    | `session.auth`, `events.subscribe`                                                                                                                |
| daemon     | `daemon.info` (with `apiVersion`), `daemon.stop`, `status`, `profile.create`                                                                      |
| settings   | `config.get`, `config.set` (a `SettingsPatch`), `relays.set`                                                                                      |
| signer     | `signer.status`, `signer.unlock` (`Secret` is `{ passphrase }` or `{ key }`), `signer.lock`, `signer.sealingKey`                                  |
| node       | `node.status`                                                                                                                                     |
| founding   | `lobby.inspect`, `lobby.create`, `lobby.join`, `lobby.get`, `lobby.list`, `lobby.start`, `lobby.leave`, `founding.begin`, `founding.get`          |
| vaults     | `vault.list`, `vault.details`, `vault.peers`, `vault.audit`, `vault.activity`, `vault.counterparties`, `vault.alias`, `vault.use`, `vault.retire` |
| tokens     | `tokens.list`, `tokens.add`, `tokens.remove`, `tokens.deployTest` (local network only)                                                            |
| proposals  | `proposals.list`, `get`, `preview`, `create`, `review`, `approve`, `reject`, `execute`, `cancel`                                                  |
| contracts  | `contracts.functions`                                                                                                                             |
| aztec      | `aztec.aliases`, `aztec.alias`: `aztec-wallet`'s aliases, the only names for addresses (Ome keeps no address book)                                |
| membership | `membership.request`, `membership.plan`, `membership.begin`, `membership.get`, `membership.list`, `membership.swapSigner`                         |
| devices    | `devices.pair`, `devices.list`, `devices.revoke`, `devices.enroll`, `devices.forward`                                                             |

- `proposals.approve` and `proposals.reject` take an optional `secret`, which unseals a locked signer for that one
  call.
- An `Intent` is one of:
  - `transfer { token, to, amount, memo }`;
  - `calls { calls: [{ to, fn, args }], memo }`;
  - `cancel { cancels, memo }`.
- **Membership** (`architecture/daemon.md`):
  1. a newcomer's daemon produces a join request (`membership.request`), signed by the newcomer's keeper;
  2. a member plans the change (`membership.plan`, taking the requests) and hands the plan string
     (`ome-rotation-v0:`) to everyone who stays and every joiner;
  3. each passes it to `membership.begin`, and their keepers reshare;
  4. the planner proposes `rotate_membership`, and members approve it with `proposals.approve`, which checks it
     against their own reshare;
  5. the keepers activate the new committee.
- `vault.retire` forces the keeper's `vault.retire`: a member removed by a rotation it took no part in can't read the
  new config to show its keeper, and destroys its share deliberately.
- The keeper's `ceremony.reshare` takes optional `secrets`, which the keeper seals to joiners, and `ceremony.status`
  returns them to a joiner in the result's `secrets`. The keeper's `vault.retire` takes an optional `force`.

## Daemon events

Every daemon event arrives as `EventEnvelope { seq: { epoch, n }, at, event }`. `epoch` is the daemon's start (it
changes on every restart); `n` counts up from 1 within it. The daemon keeps the last 1,024 envelopes.

`events.subscribe { since? }` → `Subscription { epoch, latest, reset }`:

- without `since`, the stream starts with the next event;
- with `since` (the last event a client saw), the events after it are written right after the answer, then new ones;
- when they can't be replayed (another epoch, or older than the oldest kept), the answer has `reset: true` and nothing
  is replayed: the client refetches what it shows, once.

`DaemonEvent` types, and the results each makes stale (a client refetches exactly these):

| Event                                      | What changed                                                 | Stale results                                                   |
| ------------------------------------------ | ------------------------------------------------------------ | --------------------------------------------------------------- |
| `keeper`                                   | the keeper's service state                                   | `status`                                                        |
| `keeperEvent`                              | a raw `KeeperEvent`, for views that follow the keeper itself | none (the scoped events below cover what it changes)            |
| `signer`, `node`                           | the signer's lock, the node check                            | `status`, `signer.status`, `node.status`                        |
| `lobby`, `founding`                        | a lobby or founding                                          | `lobby.get`, `lobby.list`, `founding.get`                       |
| `proposalCreated { by }`                   | a proposal arrived, made here or replicated                  | `proposals.list` of the vault; `vault.activity`                 |
| `proposalVoted { by, approve, reason }`    | a member approved or rejected                                | `proposals.get` and `proposals.review` of it; `proposals.list`  |
| `proposalReady`, `proposalBlocked`         | it has its approvals, or can no longer pass (once each)      | `proposals.get` of it; `proposals.list`                         |
| `proposalExecuted { txHash }`              | its transaction was mined                                    | `proposals.get` of it; `proposals.list`; `vault.activity`       |
| `execution`                                | an execution on this device advanced                         | `proposals.get` of it                                           |
| `received { token, amount, from, txHash }` | a transfer into the vault was found                          | `vault.activity`, `vault.counterparties`                        |
| `balanceChanged { token }`                 | a token's balance may have changed                           | `tokens.list` of the vault                                      |
| `tokenAdded`, `tokenRemoved`               | the tracked tokens                                           | `tokens.list` of the vault                                      |
| `vaultUpdated`                             | the member's record: alias, default, status                  | `status`, `vault.list`, `vault.details`                         |
| `configChanged { version }`                | the on-chain config                                          | `vault.details`; `proposals.list` and every review of the vault |
| `memberChanged`                            | who is in the vault                                          | `vault.details`, `vault.peers`, `status`                        |
| `rotation`                                 | a rotation advanced                                          | `membership.get`, `membership.list`                             |
| `handshakeAdvised`                         | counterparties should handshake again                        | `vault.counterparties`                                          |

Proposal events come from the keeper's `proposal`, `payload`, `approval`, `rejection` and `receipt` events, which the
keeper raises for what this member stores and for what members' keepers replicate alike. A proposal whose payload
hasn't arrived is announced once it does. Each carries `vault`, `proposal` and a one-line `summary`
("Transfer 40 OMET to 0xf941…12b3"). `received` is found on the daemon's 15-second sync; the first look at a vault
after a start only learns what it already received.

## Versioning

`keeper.info` and `daemon.info` report `apiVersion`, currently `0`.

- **Free within a version**:
  - a new method;
  - a new optional param;
  - a new result field;
  - a new event type;
  - a new error kind, which older clients read as `failed`.
- **Needs the next version**: anything else that changes a shape, such as:
  - renaming or removing a field or method;
  - making a param required;
  - changing a field's type.

Clients should check `apiVersion` before relying on a shape. The daemon refuses a keeper that speaks another version
when it connects; `ome-client` leaves the check to its caller.

## TypeScript

`pnpm --filter @ome-labs/api generate` does three things:

1. runs `ome-api`'s `typescript` example, which renders every declared type with ts-rs 12.0.1, `u64` as `number`;
2. writes a `KeeperMethods` and a `DaemonMethods` map, `ERROR_CODES` and `API_VERSION` into
   `packages/api/src/generated/api.ts`;
3. formats the file with oxfmt.

`src/index.ts` adds lookups (`KeeperParams<M>`, `DaemonResult<M>`, and so on), and `test/types.ts` checks the
conventions at compile time.

- The **API types** CI job regenerates the file and fails on any diff.
- `crates/api/tests/schema.rs` checks that:
  - method names are unique;
  - declared names are unique;
  - error codes round-trip;
  - shapes serialize as described above.

## ome-client

- **Session**: one authenticated connection per client.
  - Calls get increasing ids and are answered in any order, so calls on clones of a client run concurrently.
  - A reply line is limited to 8 MiB.
  - When the connection closes, every waiting call fails with `Closed`.
- **Events**: `events()` subscribes the session once and returns a stream. A stream that falls more than 1,024 events
  behind reports `Lagged`.
- **Clients**:
  - `KeeperClient::connect(socket, token)`;
  - `DaemonClient::connect(socket, token)`, or `DaemonClient::discover(profile)`.
- **Errors**: `ClientError::kind()` maps local failures onto the API kinds. A refused connection, a closed connection
  and a missing daemon are `not-ready`, so callers handle them the way they handle the service's own errors.
- **Discovery**: the Ome home is `OME_HOME`, or `~/.ome`. Each profile is `profiles/<name>/`, selected by
  `OME_PROFILE` or `default`. Its daemon writes two files there:
  - `daemon.addr`, the socket path, relative to the profile;
  - `daemon.token`.

  Discovery refuses a token file that other users can read, and a profile name that could leave the home.

- **Transport**: Unix sockets now, and named pipes on Windows. A TCP or WebSocket transport for remote keepers can
  join `transport.rs` without changing the session.

### Tests

- `tests/keeper.rs`: two real keepers on the in-memory network, each serving its RPC on a socket.
  - A wrong token is refused.
  - Errors carry their kind.
  - Concurrent calls on one session each get their own answer.
  - Both members run a DKG through typed calls, see it complete on their event streams, and agree on the transcript.
- `tests/daemon.rs`: a stand-in daemon that speaks the API envelopes.
  - A client discovers it and calls it.
  - Auth errors keep their kind.
  - Events arrive.
  - A dropped connection fails the waiting call.
  - Discovery refuses missing files, a shared token file and escaping profile names.
