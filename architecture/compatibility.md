# Compatibility

How a release changes a message, a stored record, an API or a pasted string without members doing anything but
installing it, in any order.

**Until launch, protocol `v0` stays open:** a release may still change `v0` (every member of a vault then upgrades
together), and the guards below only report. The launch release freezes `v0` and switches them to enforce
(`compat/MODE`).

## Principles

- Nothing signed is rewritten. A record keeps the bytes it was signed in, and ids, hashes and signatures always use
  those bytes.
- A release reads everything an earlier one wrote, and writes only what every member of the vault can read.
- A reader that meets something newer than it knows says so ("needs a newer release of Ome", "written by a newer
  Ome: update") instead of misreading it.
- Additions don't need a version: an extension, a new optional API field, a new event. Changes do: a protocol version,
  a file schema, an API version.

## Messages and records (`ome-wire`)

- **Extensions.** The envelope and every record stored or re-sent on its own (proposals, approvals, rejections,
  receipts, names, manifests, capabilities records, the execution request) end with `extensions`, inside their
  signed bytes: `{ id, critical, bytes }`, ids strictly increasing, at most 16 of 4 KiB. A reader ignores one it
  doesn't know unless it is critical, when the record is refused (`UnknownCriticalExtension`). None is defined yet.
- **Versions.** A signed message is `version ‖ header ‖ body ‖ extensions ‖ signer ‖ signature`; decoding reads the
  version first and parses the body with that version's encoding (`encoding.rs`), converting it to the one model the
  rest of the code uses. Signatures are checked over the version's own bytes. `encode_body_in` / `decode_body_in`
  store and read a body in a given version. Today there is one version, `v0`, and it is the model; the crate's tests
  prove the dispatch with a test-only second one.
- **A vault's version.** Each member's keeper signs a `Capabilities { vault, versions, release, at, by }` record and
  sends it on its periodic round when it changed; keepers replicate and sync it like renames, keeping each member's
  newest. A vault's messages are written in the newest version every current member speaks (`vault_version`); a member
  with no record counts as the first version, so the vault moves only once every member can read the new one, with
  no vote and no command. `vault.protocol` reports it; `require_protocol` refuses a feature that needs a newer version,
  naming the members holding it back. A paired phone says which versions its app reads; its keeper writes to it in
  the newest one both the vault and the app speak, or says the app is too old.
- **Pasted strings.** Invites, join requests, pairing offers and rotation plans carry their format in the prefix
  (`ome-invite-v0:`); payment links carry `v=0` (a link without it is format 0). A newer format is refused as needing
  a newer release, never as malformed.

## Stored data

- **Keeper store.** Migrations are append-only (`migrations.rs`). A store whose schema is past the release's migrations
  is refused (`NewerStore`). Each stored record keeps the protocol version it was written in (`version` columns,
  migration 8); reads go through the SQLite function `ome_body(version, body)`, which converts an older record to the
  current encoding, so the rest of the keeper sees one encoding and the stored bytes never change.
- **Daemon profile.** `profile.json`, each vault record and each rotation in progress carry `schema`, with an ordered
  list of migrations (`profile/schema.ts`). An older file is migrated on load and written back, the previous one kept as
  `<file>.bak-<schema>`; a newer one is refused. A file without `schema` is schema 0. Settings default when added, since
  a setting is an open record.
- **Sealed files** start with a format byte; a newer format asks for an update.
- **Phone.** Each file is kept as `{ schema, value }` and migrated as it is read; a newer one asks to update the app.

## APIs

- `daemon.info` and `keeper.info` list `capabilities` (`authwits`, `apps`, `capabilities`, …): clients check a feature
  instead of comparing versions, and a client newer than its daemon hides what the daemon lacks.
- A value a newer release sends (an intent kind, a proposal status, a simulation failure, an event) parses as its
  enum's `unknown` case, in Rust (`#[serde(other)]`) and in the daemon (an intent kind it doesn't know reads as
  `unknown` and shows by its decoded calls).
- Within an `apiVersion` additions are free; a change waits for the next one (`architecture/api.md`).

## Updates

`ome update` updates through however Ome was installed (npm, Homebrew, the install script) and restarts the profile's
daemon from the new release. The daemon reads GitHub's latest release on start and once a day (`updateCheck`, on by
default; nothing else is sent), and `ome status` and `ome doctor` name a newer one. Local files then upgrade on start,
and each vault moves to a newer protocol by itself once its members have updated.

## Guards (`compat/`)

| Guard                                                                                  | Catches                                                                                                                                 |
| -------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `compat/vectors/v<N>.json`, checked by `crates/wire/tests/frozen.rs`                   | a frozen message that no longer decodes, verifies or re-encodes; the current version's encoding changing without a new protocol version |
| `compat/fixtures/<release>/`: a keeper store, a profile, a vault record                | a release's data the current code no longer opens                                                                                       |
| `pnpm test:compat`: the CLI scenario with one member on the previous published release | releases that don't work together                                                                                                       |
| refuse-newer tests (keeper store, profile files, phone files)                          | a downgrade misreading newer data                                                                                                       |

`compat/MODE` is `report` until launch: the guards print what changed and pass, and `pnpm compat:freeze <release>`
re-freezes `v0` and writes that release's fixtures. The launch release sets it to `enforce`: from then on a frozen
version never changes, and a change fails CI until it ships as a new protocol version or a migration.

## Not covered

- The vault contract (its notes, witness layout and approval message) changes only through a contract release; how a
  vault moves to a new class without changing its address is still research (`research/compatibility.md`).
- A new Aztec rollup version is a new chain: funds move to a successor vault.
