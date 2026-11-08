# ome-net

`crates/net` moves `ome-wire` frames between keepers, relays and clients that talk to them directly. It knows
nothing about vaults, ceremonies or proposals. It delivers signed frames, says which channel authenticated each
one, decides who may connect, and stores envelopes for offline members at relays. It is native only, built on tokio
and libp2p 0.57.1.

## The boundary: `Transport`

A keeper depends only on this trait. No libp2p type crosses it.

```rust
trait Transport {
    fn send(&self, peer: Peer, frame: Vec<u8>) -> SendFuture<'_>;      // best effort; protocols retry
    fn set_admission(&self, members: Vec<Peer>, lobby_open: bool);      // who may connect
    fn peer_status(&self, peers: &[IdentityPublic]) -> Vec<PeerStatus>; // connected, relayed, last seen, RTT
}
// inbound frames arrive as Inbound { channel: Channel, bytes } on the receiver returned at start
```

- `Peer` is a manifest member: identity, transport key and addresses.
- `Channel` says what authenticated a frame:
  - `Peer(identity)`: a Noise-authenticated connection, or the in-process network;
  - `Key(transport key)`: a sealed mailbox envelope.
- **The keeper requires a frame's signer to match its channel**, so one member can't pass off another member's
  messages, even over a legitimate connection.

There are two implementations:

- **`MemoryNetwork` / `MemoryTransport`**: a real in-process network with the same admission. A filter can drop
  frames to model partitions, and `leave` models a crashed keeper. Keeper tests run many keepers on it, and an app can
  embed a keeper with it.
- **`Libp2pTransport`**: the node described below.

## Identity

A keeper's libp2p keypair comes from its `ome-wire` `KeeperIdentity` seed, so its peer id _is_ its keeper identity.
Ed25519 peer ids inline the public key, so `identity_of(peer_id)` recovers the identity without a lookup. The Noise
handshake therefore authenticates the keeper, and its frames are signed by the same key.

## Lanes and framing

| Lane     | Protocol          | Carries                                                       |
| -------- | ----------------- | ------------------------------------------------------------- |
| ECDH     | `/ome/ecdh/0`     | ECDH requests and responses                                   |
| ceremony | `/ome/ceremony/0` | manifests, DKG and reshare rounds, complaints, aborts, echoes |
| sync     | `/ome/sync/0`     | proposals, payload requests, payloads, votes, receipts, sync  |
| lobby    | `/ome/lobby/0`    | lobby join, state, plan and leave                             |
| mailbox  | `/ome/mailbox/0`  | relay mailbox requests and responses                          |

`Lane::of(frame)` picks the lane from the decoded message. A frame that doesn't decode goes to the ceremony lane,
where the receiver rejects it.

- **Codec**: each request and response is a `u32` big-endian length followed by the bytes.
  - The length is checked **before** the body is allocated.
  - Keeper lanes allow requests up to 1 MiB (`MAX_MESSAGE_BYTES`) and acknowledgements up to 64 bytes. The mailbox
    lane allows 4 MiB both ways.
  - A truncated stream is an error, not a hang.
- **Versioning**: the trailing number of each protocol name is the `ome-wire` protocol version. A keeper that speaks
  several versions offers each name, and libp2p's protocol negotiation picks one both peers speak. Identify announces
  `/ome/0`.

## The libp2p node

- **Transports**: QUIC (`/udp/…/quic-v1`) and TCP with Noise and Yamux, plus DNS resolution.
- **NAT traversal**:
  - the node reserves a circuit at each configured relay, so a keeper behind NAT stays dialable at
    `<relay>/p2p-circuit`;
  - DCUtR then tries to upgrade a circuit to a direct connection by hole punching.
- **Health**: ping supplies the RTT; connection events supply `connected`, `relayed` (every connection is a circuit)
  and `last_seen`; idle connections close after 2 minutes.
- **Startup**: `start` returns once every listener is bound and every relay accepted a reservation, or after the ready
  timeout.
- **Advertised addresses**: direct addresses first, then the relay circuits, which are always kept as the fallback;
  at most 8, the manifest's limit.
  - Loopback is advertised only by a loopback-only node: advertised to another host, it would dial the dialer itself
    and fail the whole dial.
  - A peer address that equals one of our own listeners is skipped.
- **No local discovery (mDNS)**: it would broadcast keeper peer ids on every network the device joins. Peers come
  only from manifests.

## Admission

`Admission` is plain logic, shared by both transports:

- members of the committees and ceremonies the node serves, and its configured relays, may connect;
- while the node hosts an **open lobby**, anyone may connect, but a stranger's frames are delivered only on the lobby
  lane, and only when the frame itself is a lobby message. They are limited to 10 requests a minute per stranger, with
  at most 4,096 strangers tracked.

On libp2p, the gate refuses an unadmitted connection **before any protocol runs**, inbound and outbound. Calling
`set_admission` without a member, or closing the lobby, disconnects the peers the new admission refuses, so a
removed member's keeper loses its connection at once. Sending to a peer the sender doesn't admit fails with
`NotAdmitted`.

## Mailboxes

`Mailbox` is where envelopes for offline members wait:

- **`RelayMailbox`** uses the node's configured relays:
  - deposit goes to every relay and succeeds if any one stores the envelope;
  - fetch merges every relay's answer, without duplicates;
  - delete goes everywhere;
  - requests are split at the wire limits (128 mailboxes, 1,024 ids).
- **`MemoryMailbox`** keeps the same contract in process.

Mailbox ids come from `ome_crypto::sealing::mailbox::mailbox_id`: one per sender–recipient pair per day, so the
relay can't link them.

## What each party sees

| Observer           | Sees                                                                                 | Never sees                                          |
| ------------------ | ------------------------------------------------------------------------------------ | --------------------------------------------------- |
| a relay            | IP addresses and peer ids of whoever connects, mailbox ids, sizes, times, ciphertext | vault addresses, signer keys, thresholds, plaintext |
| a network observer | that two addresses speak Noise or QUIC, sizes and times                              | peer ids inside Noise, contents                     |
| another keeper     | frames from admitted peers; strangers only on an open lobby's lane                   | anything from peers it doesn't admit                |

Keeper peer ids are network identities. They are never written on chain.

## Dependencies

`libp2p` 0.57.1 (QUIC, TCP, DNS, Noise, Yamux, identify, ping, request-response, relay, DCUtR), `tokio` 1.54.1,
`futures` 0.3.34 and `tracing` 0.1.44.

`crates/deny.toml` accepts one advisory: RUSTSEC-2024-0436. `paste` is unmaintained, reached at compile time through
libp2p → if-watch → netlink-packet-core, and has no fixed release. libp2p 0.57 still uses the previous RustCrypto
generation, so `cargo deny` reports those crates as duplicates. Duplicates are warnings, kept visible on purpose.

## Tests

- `tests/framing.rs`: round trip; an oversized prefix is refused before the body is read; truncation is an error; each
  message kind travels on its lane.
- `tests/gate.rs`: members and relays are admitted, strangers aren't; removal closes admission; an open lobby admits
  strangers on its lane only; the per-minute rate limit.
- `tests/memory.rs`: authenticated channels and peer status; admission at both ends; partitions; departed peers.
- `tests/libp2p.rs`:
  - real loopback nodes exchange frames over TCP+Noise and over QUIC;
  - a keeper with no direct address is reached through a relay circuit (reported as relayed);
  - the receiver's gate refuses a peer it doesn't admit;
  - an open lobby admits a stranger on the lobby lane only.
- `tests/mailbox_client.rs`: deposit, fetch, merge and delete across two test relays; `MemoryMailbox` keeps the same
  contract.

The test relay in `tests/support/relay.rs` combines the circuit relay server with an in-memory mailbox store.
`ome-relay` replaces it in phase 8.
