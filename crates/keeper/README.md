# ome-keeper

An Ome member's keeper: the always-on service that holds the member's viewing share for each vault and never lets it
out. It is both a library and the `ome-keeper` binary.

The keeper:

- runs the DKG and reshare ceremonies with the other members' keepers;
- answers threshold ECDH requests with DLEQ-proven partials, and gathers them for its own member;
- activates, holds or destroys shares as the vault's on-chain committee changes, acting only on configs it can verify;
- stores and replicates proposals, payloads and votes, hosts and joins founding lobbies, and uses relay mailboxes for
  offline members;
- pairs its member's signing devices, and serves its member's daemon over a local JSON-RPC socket (`rpc`), or over TLS
  pinned by fingerprint when it runs on the member's own server.

`Keeper::start` takes its transport, clock and storage from outside, so tests run many keepers in one process.

## Running it

Members do not start it by hand: `omed` runs it with a master key from the OS keychain. On a server:

```sh
OME_KEEPER_PASSPHRASE_FILE=/run/secrets/keeper-passphrase \
  ome-keeper run --data-dir /data --listen /ip4/0.0.0.0/tcp/4001 --rpc-listen 0.0.0.0:7445
```

The `ghcr.io/ome-labs/ome-keeper` image and `relay/deploy/ome-keeper.service` package this.

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-keeper
```

## Learn more

[architecture/keeper.md](https://github.com/ome-labs/ome/blob/main/architecture/keeper.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
