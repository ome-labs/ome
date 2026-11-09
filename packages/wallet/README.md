# @ome-labs/wallet

The Aztec wallet for [Ome](https://ome.network) vaults: a PXE whose key store never holds a vault's incoming viewing
secret, vault deployment, and k-of-n proposals from preparation to execution.

The PXE is [`@ome-labs/pxe`](https://www.npmjs.com/package/@ome-labs/pxe), Aztec's PXE with one guarded change: an
account's viewing secret is used only through `ThresholdKeyStore`, which obtains `address_secret · P` from `t`
members' keepers by threshold ECDH, and checks each keeper's proof before using its part.

For integrators building their own member software. Members use [`@ome-labs/ome`](https://www.npmjs.com/package/@ome-labs/ome).

## Install

```sh
npm install @ome-labs/wallet    # Node 24 or later
```

## Use

```ts
import { createAztecNodeClient } from "@aztec-labs/aztec.js/node";
import { KeeperClient, OmeWallet } from "@ome-labs/wallet";

const node = createAztecNodeClient("http://localhost:8080");
const wallet = await OmeWallet.create(node, { pxe: { dataDirectory: "./pxe" } });

// Register a vault with the committee its keeper serves, then read and spend through it.
const keeper = await KeeperClient.connect(socketPath, token);
await wallet.registerVault(deployment, viewing);
const config = await wallet.getVaultConfig(vault);
```

`deriveVault` computes a vault's address and keys from its founding setup; `prepareVaultTx`, `tallyVaultTx`,
`previewVaultTx` and `executeVaultTx` take a proposal from draft to a mined transaction.
`@ome-labs/wallet/testing` holds helpers for tests that handle raw secrets; never use it in production.

## Documentation

- [PXE integration](https://github.com/ome-labs/ome/blob/main/architecture/pxe-integration.md): the threshold key store and the PXE change
- [Protocol](https://github.com/ome-labs/ome/blob/main/architecture/protocol.md) and [cryptography](https://github.com/ome-labs/ome/blob/main/architecture/crypto.md)

## Versions

Released together with every other `@ome-labs` package, with one version. Built for Aztec 6.0.0 (`AZTEC_VERSION`):
use it with `@aztec-labs/*` 6.0.0.

## License

Apache-2.0
