# @ome-labs/daemon

`omed`, an [Ome](https://ome.network) member's background service: it hosts the member's Aztec wallet, keeps their
signing key sealed, supervises their keeper, and serves the daemon API over an owner-only socket.

Members get `omed` through [`@ome-labs/ome`](https://www.npmjs.com/package/@ome-labs/ome), which starts it as
needed. Install this package directly to build an app or a service on top of a member's daemon.

## Install

```sh
npm install @ome-labs/daemon    # Node 24 or later
```

## Use

Talk to a running daemon with `DaemonClient`. Methods, params and results are typed by
[`@ome-labs/api`](https://www.npmjs.com/package/@ome-labs/api).

```ts
import { readFileSync } from "node:fs";

import { DaemonClient, profilePaths } from "@ome-labs/daemon";

const paths = profilePaths("alice"); // under OME_HOME, by default ~/.ome
const client = await DaemonClient.connect(paths.socket, readFileSync(paths.token, "utf8").trim());

const vaults = await client.call("vault.list", {});
const stop = await client.subscribe((event) => console.log(event));
```

Run the daemon itself with the `omed` binary:

```sh
omed --profile alice [--home ~/.ome]
```

## Documentation

- [The daemon](https://github.com/ome-labs/ome/blob/main/architecture/daemon.md): profiles, the sealed signer, keeper supervision, networks
- [The daemon API](https://github.com/ome-labs/ome/blob/main/architecture/api.md): transport, sessions and every method

## Versions

Released together with every other `@ome-labs` package, with one version. This release works with Aztec 6.0.0.

## License

Apache-2.0
