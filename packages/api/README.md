# @ome-labs/api

TypeScript types for the APIs of an [Ome](https://ome.network) member's keeper and daemon: every method, its params
and result, every event and error. They are generated from the Rust definitions in `ome-api`, so the TypeScript and
Rust sides cannot drift.

For integrators writing a client of `omed` or `ome-keeper`. The package holds types only (and no runtime code).

## Install

```sh
npm install @ome-labs/api
```

## Use

```ts
import type { DaemonMethod, DaemonParams, DaemonResult, DaemonEvent } from "@ome-labs/api";

async function call<M extends DaemonMethod>(
  method: M,
  params: DaemonParams<M>,
): Promise<DaemonResult<M>> {
  // JSON-RPC 2.0 over the daemon's socket, one object per line
}

const vaults = await call("vault.list", {});
```

`KeeperMethod`, `KeeperParams` and `KeeperResult` do the same for the keeper's API.
[`@ome-labs/daemon`](https://www.npmjs.com/package/@ome-labs/daemon) ships a ready client, `DaemonClient`.

## Documentation

[The API](https://github.com/ome-labs/ome/blob/main/architecture/api.md): transport, sessions, encodings and every method.

## Versions

Released together with every other `@ome-labs` package, with one version. A daemon and a client of the same version
always agree.

## License

Apache-2.0
