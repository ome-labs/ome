# ome-relay

`crates/relay` is the **untrusted helper node** anyone can run: Ome, a member, a hosting provider. It does two jobs:

1. **Circuit relay**: a keeper behind NAT reserves a slot and stays dialable at `<relay>/p2p-circuit`. `ome-net`
   then tries to upgrade the circuit to a direct connection by hole punching (DCUtR).
2. **Mailbox store**: other keepers leave sealed envelopes for an offline member, and its keeper collects them when it
   returns.

It only ever holds ciphertext under opaque ids. Circuits are encrypted end to end between keepers (Noise), envelopes
are sealed to the recipient, and every frame is signed by its sender. A relay therefore can't read, forge, or tell
which vault anything belongs to. If it disappears, keepers use their other relays, connect directly, or catch up
through sync.

## The node

- **Transports**: QUIC and TCP with Noise and Yamux, plus DNS, identify (`/ome/0`) and ping. The relay's Ed25519
  identity lives in `<data>/identity.key` (`0600`); its peer id goes into keepers' `--relay <address>/p2p/<peer id>`.
- **Circuit limits**, sized for ceremonies and ECDH batches (libp2p's defaults are 2 minutes and 128 KiB):

  | Limit             | Default                  |
  | ----------------- | ------------------------ |
  | reservations      | 1,024                    |
  | circuits          | 256 at once, 16 per peer |
  | circuit lifetime  | 30 minutes               |
  | bytes per circuit | 64 MiB                   |

- **Mailbox lane**: `/ome/mailbox/0` with `ome-net`'s `FrameCodec::mailbox()`.
  - Requests are decoded with `ome-wire`'s bounded `decode_request` and answers encoded with `encode_response`. An
    answer that would not fit is replaced by `Refused(Malformed)`, never sent truncated or empty.
  - Answers are `Stored`, `Refused(reason)`, `Envelopes` or `Deleted`.
- **Open or private**: open by default, protected by quotas and a per-peer deposit limit. With `--allow-peer <peer id>`
  (repeatable), only those keepers may reserve circuits or use mailboxes: a private relay for one team. A member's
  paired signing device is a peer too, so a private relay must also allow the device's peer id.
- **Startup**: the node waits until every listener has reported its addresses, then settles briefly, so every
  interface and both transports are announced. It then prints `{"ready": {"peerId", "addresses"}}` on stdout.

## Mailbox store

SQLite in WAL mode with `secure_delete` on. A row holds the envelope id, mailbox id, expiry, sealed bytes, size and
stored-at time. **Who deposited it is never recorded.** There is no other table.

| Rule              | Default                                     | Refusal         |
| ----------------- | ------------------------------------------- | --------------- |
| envelope size     | 1 B – 512 KiB (`MAX_ENVELOPE_BYTES`)        | `TooLarge`      |
| expiry            | in the future, at most 14 days ahead        | `BadExpiry`     |
| per mailbox       | 512 envelopes, 32 MiB                       | `QuotaExceeded` |
| in total          | 1 GiB                                       | `QuotaExceeded` |
| deposits per peer | 3,600 an hour, at most 65,536 peers tracked | `QuotaExceeded` |
| allow list        | when set                                    | `NotAllowed`    |
| request decoding  | `ome-wire` bounds                           | `Malformed`     |

- **Deposit** is idempotent: the same envelope gets the same id.
- **Fetch** returns unexpired envelopes of the requested mailboxes, oldest first, up to 3 MiB and at most 1,024
  envelopes (`MAX_FETCH_ENVELOPES`), so an answer always fits the wire. The keeper deletes what it delivered and
  fetches again.
- **Delete** is by envelope id.
- **Expiry**: a sweep every minute removes expired envelopes.

Anyone who knows a mailbox id can fetch from it. Mailbox ids are derived from the pair's X25519 shared secret and the
day, so only the two members can compute them. Envelopes are sealed, so a fetch by anyone else returns only
ciphertext.

## Health and metrics

With `--health <address>` (off by default), the relay serves plain HTTP:

- `GET /health`: `{"status": "ok", "peerId", "envelopes", "bytes"}`;
- `GET /metrics`: Prometheus text. The counters are deposits, fetches, envelopes delivered and deleted, reservations,
  circuits, and refusals by reason; the gauges are active circuits, envelopes held and bytes held.

Metrics are aggregates only: no peer id, mailbox id or envelope id is ever recorded. Expose the endpoint to your
monitoring, not to the internet.

## Running a relay

- **Binary**: `ome-relay run --data-dir <dir>`, with:
  - `--listen` (default `/ip4/0.0.0.0/udp/4001/quic-v1` and `/ip4/0.0.0.0/tcp/4001`);
  - `--external <multiaddr>` for the public address keepers dial;
  - `--health`;
  - `--allow-peer`;
  - `--max-ttl-days` and `--max-total-mib`;
  - `--log-format text|json`.

  `ome-relay peer-id --data-dir <dir>` prints the peer id, creating the identity if needed.

- **Image** (`relay/deploy/Dockerfile`, built from `crates/`): `ome-node` carries both `ome-relay` and
  `ome-keeper`, so relay operators and members running a server keeper use the same image.
  - It runs as a non-root user with an `0700` `/data` volume and exposes 4001/udp, 4001/tcp and 9090.
  - Its health check probes `/health` with bash (Debian's `sh` has no `/dev/tcp`).
  - It runs the relay by default.
  - CI builds it and checks that the relay reports healthy.
- **systemd**: `relay/deploy/ome-relay.service` and `ome-keeper.service` run with `DynamicUser`, an `0700`
  `StateDirectory`, `NoNewPrivileges`, `ProtectSystem=strict`, no capabilities and restricted address families. The
  keeper's passphrase comes from a root-only systemd credential.

**Operating a public relay.** Run two or three in different regions, since keepers deposit to and fetch from all of
their relays. Publish what is logged, ideally aggregate metrics only. Keep the default relay list overridable in
clients.

## Tests

- `tests/store.rs`:
  - deposit, fetch and delete per mailbox;
  - each limit refused with its reason;
  - expired envelopes neither served nor kept;
  - fetch answers stop at `MAX_FETCH_ENVELOPES`;
  - the database records no depositor.
- `tests/relay.rs`:
  - a keeper with no direct address is reached through the real relay's circuit;
  - a private relay serves only its allow list.
- `tests/mailbox.rs`: `ome-net`'s `RelayMailbox` against two real relays (deposit everywhere, merged fetch, delete
  everywhere, sealed bytes held exactly), a private relay refuses a stranger's deposit, and a client-mode node (a
  signing device) uses the mailbox without listening or reserving a circuit.
- `tests/health.rs`: `/health` and `/metrics` answer, and metrics name no peer, mailbox or envelope id.
