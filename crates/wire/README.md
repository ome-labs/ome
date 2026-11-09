# ome-wire

The bytes Ome components exchange: keeper with keeper, a daemon with its own keeper, keepers with a relay.

Every message travels as a `SignedMessage`: a protocol version, a `Header` (scope, epoch, id, timestamp) and a
`Message` body, signed by the sending keeper's Ed25519 identity (`KeeperIdentity`) over the canonical `postcard`
encoding.

- `encode` / `decode` accept only canonical bytes, enforce the `limits` and the supported `version` range, and check
  per-message bounds.
- `ReplayGuard` rejects duplicates, other epochs and stale timestamps.
- `messages` and `types` define every body: ceremonies, threshold ECDH requests and partials, proposals, votes,
  lobbies and mailbox envelopes.

No transport lives here (see `ome-net`). The decoders take untrusted bytes and have fuzz targets in `wire/fuzz`
(nightly, `cargo fuzz`).

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-wire
```

## Learn more

[architecture/wire.md](https://github.com/ome-labs/ome/blob/main/architecture/wire.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
