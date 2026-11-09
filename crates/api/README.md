# ome-api

The local APIs of an Ome member's services, defined once in Rust and generated as TypeScript (`@ome-labs/api`).

- `keeper`: what a member's daemon calls on its own keeper.
- `daemon`: what the CLI and apps call on the member's daemon (`omed`).

Both are JSON-RPC 2.0 over an owner-only local socket (`jsonrpc`). Every method is a marker type implementing
`Method`; its params and result are the types here. Bytes are `0x` hex (`Hex32`, `HexBytes`), points are `{ x, y }`,
amounts are decimal strings (`Amount`), times are Unix seconds, and a missing value is `null`. Errors are `RpcError`
with a stable `ErrorKind`.

`RELEASE_VERSION` is the Ome release this build belongs to; the keeper and daemon report it.

## TypeScript

```sh
pnpm --filter @ome-labs/api generate   # packages/api/src/generated/api.ts from these types
```

CI regenerates it and fails if the committed file differs.

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-api
```

## Learn more

[architecture/api.md](https://github.com/ome-labs/ome/blob/main/architecture/api.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
