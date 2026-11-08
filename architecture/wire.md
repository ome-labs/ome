# ome-wire

`crates/wire` defines the bytes Ome components exchange:

- keepers with keepers;
- a daemon with its own keeper;
- keepers with a relay.

It is data plus validation only. Transport (libp2p streams, framing) lives in `ome-net`. Protocol rules, such as
which messages are valid in which state or how an approval signature is checked, live in `ome-protocol`. The crate
depends on `serde`, `postcard`, `ed25519-dalek`, `sha2`, `base64`, `zeroize` and `ome-crypto`, and builds for
`wasm32-unknown-unknown`.

## Encoding

Every value is encoded with `postcard`. Only the **canonical** encoding is accepted. The decoder re-encodes what it
parsed and refuses the input unless the bytes match. postcard on its own would accept overlong varints, so without
this check one message could have several byte forms. With it, every message has exactly one byte form, which makes
signatures and byte-level deduplication sound. Trailing bytes are refused.

Decoding checks the total length before parsing. After parsing it checks every count, length and range against
`limits` (the table below). Converting a message to `ome-crypto` types (`WirePoint → Point`, `WirePartial →
Partial`, …) checks the cryptographic content, such as whether a point is on the curve and whether a field element
is canonical.

`Message` variants are encoded by position. Within a protocol version, variants are only appended; none are
reordered or removed.

## Signed messages

```text
SignedMessage = { version: u16, header: Header, body: Message, signer: IdentityPublic, signature: Bytes64 }
Header        = { scope: [u8; 32], epoch: u32, id: [u8; 16], created_at: u64 }
signature     = Ed25519(keeper identity, "ome.wire.v0" ‖ canonical(version, header, body, signer))
```

- `scope` is the vault address, or the session or lobby id before the vault exists.
- `epoch` is the committee epoch. Every routing field is signed, so a relay cannot move a message to another vault
  or epoch.
- Verification uses `verify_strict`, which refuses small-order keys and non-canonical signatures. `verify_from`
  also checks that the signer is the member the manifest names.
- The keeper identity (`KeeperIdentity`) is an Ed25519 key separate from the member's Aztec signing key. It is
  zeroized on drop and printed only as its public key.
- `ReplayGuard` refuses:
  - other epochs;
  - timestamps outside a skew window;
  - an `(signer, id)` pair seen before.

  It keeps a bounded number of ids. An id that has been evicted is older than the window by then, so replaying it
  fails as stale.

## Versions

- Every signed message carries its protocol version. `decode` accepts any version in `version::SUPPORTED`, which is
  `0..=0` today.
- A keeper advertises the versions it speaks in its `KeeperContact` (`versions: VersionRange`).
- When a vault is founded, `version::negotiate` picks the newest version every founder's keeper speaks. The
  `LobbyPlan` records it, and the plan is refused unless every member's range contains it.
- The vault's `Manifest` pins that `protocol_version`. Members sign with the manifest's version, not their newest.
- **Upgrade path:** a release that changes a wire format or domain tag adds a version and keeps speaking the old
  one. A vault moves to the new version only once every member supports it. A mismatch fails before a ceremony
  starts, and the error names the version.

## Messages

| Module      | Messages                                                                                                                                                                                                | Used for                                              |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| `ceremony`  | `Manifest`, `DkgRound1`, `DkgRound2`/`ReshareSubShare` (`SealedShare`), `DkgConfirm`/`ReshareConfirm`, `ReshareDeal`, `ReshareSecrets` (`SealedSecrets`), `Complaint`, `Justification`, `Abort`, `Echo` | the committee, creating and resharing the viewing key |
| `ecdh`      | `EcdhRequest`, `EcdhResponse` (`WirePartial` with DLEQ proof)                                                                                                                                           | threshold decryption                                  |
| `proposals` | `Proposal`, `PayloadRequest`, `Payload` (`ProposalPayload`), `Approval`, `Rejection`, `ExecutionReceipt`                                                                                                | spending                                              |
| `sync`      | `SyncRequest` (`VoteRef`)                                                                                                                                                                               | catching up after being offline                       |
| `lobby`     | `LobbyJoin`, `LobbyState`, `LobbyPlan`, `LobbyLeave`; `Invite` (string form, not a message)                                                                                                             | founding a vault                                      |
| `mailbox`   | `MailboxRequest`, `MailboxResponse`, `RelayEnvelope`, `StoredEnvelope` (unsigned)                                                                                                                       | store-and-forward through an untrusted relay          |
| `device`    | `DevicePair`, `DevicePaired`, `DeviceReview`, `DeviceVote` (`Ballot`), `DeviceVoted`, `DeviceUnpair`; `PairOffer` (string form, not a message)                                                          | a member's paired signing device                      |

`Echo` forwards a dealer's signed broadcast verbatim. If a dealer signed two different broadcasts, that proves it
equivocated.

### Proposals and payloads

A `Proposal` carries:

- `config_version`, `outer_hash`, `expires_at`, `tx_nonce`, `fee_method`;
- a summary and the proposer;
- a `PayloadRef { hash, len }`.

The Aztec execution request travels on its own as a `Payload` message, fetched by hash with a `PayloadRequest`.

```text
proposal id  = SHA-256("ome.proposal.v0" ‖ canonical(proposal))
payload hash = SHA-256("ome.proposal.payload.v0" ‖ payload bytes)
```

This split has three benefits:

- Proposals stay small enough to gossip, sync and relay cheaply.
- A member that already holds a payload never downloads it again.
- `len` lets a receiver refuse an oversized payload before fetching it.

The proposal id covers the payload reference, so an approval is bound to one payload. Before signing, a member's
daemon fetches the payload, checks `PayloadRef::matches`, and recomputes `outer_hash` from it.

Approvals and rejections carry the Schnorr signature as the contract's four limbs (`WireSignature`). Checking them
needs the approval message, a protocol rule, so that check lives in `ome-protocol`.

### Invites

An invite's string form is `ome-invite-v0:` followed by base64url, without padding, of the canonical bytes. Decoding
refuses base64 with non-zero trailing bits and non-canonical bytes, so every invite has exactly one string form. The
join secret is zeroized on drop and left out of `Debug`.

### Pairing offers and device messages

A pairing offer's string form is `ome-pair-v0:` followed by base64url of the canonical bytes, with the same canonical
rules as invites. It holds the keeper's identity and transport key, the relays, a one-time 32-byte secret (zeroized,
left out of `Debug`), an expiry, the member's name and the network label. It must name at least one relay.

Device messages are ordinary signed messages: the device signs with its own Ed25519 identity. A pairing's messages
use the header scope of the offer id zero-padded to 32 bytes; reviews and votes use the vault address. A review
carries the manifest, the config note (`DeviceConfig`), the chain (`DeviceChain`), the proposal and its payload, up to
16 labels, the daemon's simulation outcome (up to 4 KiB) and, for a rotation, the rotation the member's keeper
reshared. The flows are in `architecture/mobile.md`.

### Mailbox

Relay messages are not signed. They are canonical postcard with their own limits, checked in both directions, so a
keeper also refuses an oversized answer from a dishonest relay. The relay sees only:

- opaque mailbox ids (derived by `ome_crypto::sealing::mailbox::mailbox_id`);
- expiry times;
- ciphertext.

```text
envelope id = SHA-256("ome.mailbox.envelope.v0" ‖ canonical(envelope))
```

## Limits

| Limit                                                       | Value               |
| ----------------------------------------------------------- | ------------------- |
| signed message or body                                      | 1 MiB               |
| members                                                     | 8                   |
| addresses per keeper / address length                       | 8 / 256 B           |
| sealed share ciphertext                                     | 256 B               |
| echoed frame                                                | 64 KiB              |
| ECDH points per request                                     | 4,096               |
| proposal payload                                            | 256 KiB             |
| payloads per request                                        | 64                  |
| proposal summary / rejection reason                         | 4 KiB / 280 B       |
| sync records per kind                                       | 4,096               |
| names                                                       | 64 B                |
| invite string                                               | 4,096 characters    |
| sealed founding secrets                                     | 1,040 B             |
| relay envelope                                              | 512 KiB             |
| mailboxes per fetch / envelopes per answer / ids per delete | 128 / 1,024 / 1,024 |
| mailbox message                                             | 4 MiB               |

## Domain tags

| Tag                       | Used for                  |
| ------------------------- | ------------------------- |
| `ome.wire.v0`             | signed-message signatures |
| `ome.proposal.v0`         | proposal ids              |
| `ome.proposal.payload.v0` | payload hashes            |
| `ome.mailbox.envelope.v0` | relay envelope ids        |
| `ome-invite-v0:`          | invite string prefix      |
| `ome-pair-v0:`            | pairing offer prefix      |

## Tests

- `tests/codec.rs`: every message kind round-trips, signed and unsigned. It also checks that variant tags follow
  declaration order, and that decoding refuses trailing bytes, every truncation, overlong varints, unknown variants,
  unsupported versions, oversized input, and out-of-bounds fields.
- `tests/signed.rs`: flipping any byte is refused; the expected signer is enforced.
- `tests/replay.rs`: replay protection.
- `tests/version.rs`: version negotiation.
- `tests/messages.rs`: conversions to `ome-crypto` types, manifest faults, and payload binding.
- `tests/lobby.rs` and `tests/mailbox.rs`: invites, lobby plans and the relay protocol.
- `tests/device.rs`: pairing offers round-trip, need a relay, and keep their secret out of `Debug`.
- `tests/vectors.rs`: pins the encoding of every message kind, the invite and pairing offer strings and the mailbox messages in
  `tests/vectors/wire.json`. The TypeScript and wasm encoders check against the same file. Regenerate with
  `OME_UPDATE_VECTORS=1 cargo test -p ome-wire --test vectors` and review the diff.
- `fuzz/` (nightly, `cargo +nightly fuzz run decode`): arbitrary bytes never panic a decoder, and any accepted input
  re-encodes to exactly the same bytes. It is seeded from the vectors.
