# @ome-labs/pxe

Aztec's PXE, `@aztec-labs/pxe` 6.0.0, with one change: it can use a key store that never holds an account's whole
incoming viewing secret. [Ome](https://ome.network) vaults are read by threshold ECDH across their members' keepers,
so the PXE asks the key store for each `address_secret · P` instead of computing it from a secret it would otherwise
read.

Accounts with Aztec's stock key store behave exactly as with `@aztec-labs/pxe`: every changed site is guarded and
falls back to the stock code.

```ts
import { createPXE, getPXEConfig } from "@ome-labs/pxe/server";

const pxe = await createPXE(node, getPXEConfig(), {
  createKeyStore: (store) => new MyThresholdKeyStore(store),
});
```

`createKeyStore` is the only new option. Everything else is Aztec's API, unchanged.

## Versions

`6.0.0-ome.N` is Aztec's `6.0.0` with revision `N` of the change. Use the exact version: it is a prerelease, so no
`^6` range selects it.

## Where it comes from

This package is built from Aztec's published `@aztec-labs/pxe` and
[`patches/@aztec-labs__pxe@6.0.0.patch`](https://github.com/ome-labs/ome/blob/main/patches/%40aztec-labs__pxe%406.0.0.patch)
in the Ome repository, where each change is marked `// ome:`. The design is in
[architecture/pxe-integration.md](https://github.com/ome-labs/ome/blob/main/architecture/pxe-integration.md).

## License

Apache-2.0, like the Aztec code it modifies. See `NOTICE`.
