# ome-client

Typed clients of an Ome member's local services, over the APIs in `ome-api`.

- `KeeperClient`: the keeper, as the daemon uses it.
- `DaemonClient`: the daemon, as the CLI and apps use it. A `Profile` of the `Home` (`~/.ome` by default) finds its
  running daemon from the address and token files it writes.

A client is one authenticated session. Calls take a method marker and its params and return its result; calls on
clones of a client share the session and run concurrently. `Events` streams the daemon's notifications.

```rust
let home = ome_client::Home::from_env()?; // OME_HOME, else ~/.ome
let daemon = ome_client::DaemonClient::discover(&home.profile("alice")?).await?;
let mut events = daemon.events().await?;
```

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-client
```

## Learn more

[architecture/api.md](https://github.com/ome-labs/ome/blob/main/architecture/api.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
