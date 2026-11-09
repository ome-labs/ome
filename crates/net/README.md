# ome-net

How Ome keepers, relays and clients reach each other.

- `Transport` is the boundary a keeper depends on: send a frame to a member, receive frames with the channel that
  authenticated them, set who may connect, read reachability. `MemoryNetwork` implements it in process for tests;
  `Libp2pTransport` over QUIC or TCP with Noise, relay circuits and hole punching.
- `framing` holds the stream protocols (one per conversation, versioned by name) and the length-prefixed codec.
- `Admission` decides who may connect, and which lane a stranger may use while a founding lobby is open.
- `Mailbox` stores sealed envelopes for offline members, at relays (`RelayMailbox`) or in memory (`MemoryMailbox`).

Frames are always `ome-wire` signed messages, so authenticity never depends on the transport: the keeper checks that
a frame's signer is the peer its channel authenticated. Native only.

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-net
```

## Learn more

[architecture/networking.md](https://github.com/ome-labs/ome/blob/main/architecture/networking.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
