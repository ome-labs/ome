# ome-relay

Untrusted infrastructure for Ome keepers: a libp2p circuit relay v2 server (with the rendezvous for hole punching), so
keepers behind NAT reach each other, and a mailbox of sealed envelopes under opaque ids for members who are offline.

It never sees a plaintext, an identity tied to a vault, or a vault address. It does see which peers connect and when.

## Running it

```sh
ome-relay peer-id --data-dir ./relay   # creates the identity if needed and prints the peer id
ome-relay run --data-dir ./relay --health 127.0.0.1:9090
```

Mailbox size and lifetime are bounded (`--max-total-mib`, `--max-ttl-days`). `/health` and Prometheus metrics are on
the health address. The `ghcr.io/ome-labs/ome-relay` image (built from `relay/deploy/Dockerfile`) and
`relay/deploy/ome-relay.service` package it.

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-relay
```

## Learn more

[architecture/relay.md](https://github.com/ome-labs/ome/blob/main/architecture/relay.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
