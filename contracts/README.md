# @ome-labs/contracts

The compiled Aztec contracts of [Ome](https://ome.network):

- `OmeVault.json`: the vault, an Aztec account contract whose `entrypoint` runs a payload only when its witness
  carries valid signatures from at least `k` registered members. Signers, thresholds and the viewing committee are
  kept in private notes, never in public storage or arguments.
- `OmeVaultPreview.json`: the vault with an entrypoint that needs no approvals, used only to simulate a proposal for
  review. Never deploy it.

For integrators and explorers that need the artifacts. Written in Noir with Aztec.nr 6.0.0.

## Install

```sh
npm install @ome-labs/contracts
```

## Use

```ts
import { loadContractArtifact } from "@aztec-labs/stdlib/abi";
import compiled from "@ome-labs/contracts/OmeVault.json" with { type: "json" };

const artifact = loadContractArtifact(compiled);
```

[`@ome-labs/wallet`](https://www.npmjs.com/package/@ome-labs/wallet) exports the loaded artifact as `vaultArtifact()`.

## Documentation

- [Protocol](https://github.com/ome-labs/ome/blob/main/architecture/protocol.md): the vault's functions, notes and witness layout
- [Wire formats](https://github.com/ome-labs/ome/blob/main/architecture/wire.md): the approval message and committee commitment shared with Rust and TypeScript
- Source: [`contracts/`](https://github.com/ome-labs/ome/tree/main/contracts)

## Versions

Released together with every other `@ome-labs` package, with one version. Compiled with Aztec 6.0.0.

## License

Apache-2.0
