# @ome-labs/wasm

[Ome](https://ome.network)'s Rust core as WebAssembly, for Node and browsers: threshold viewing (checking and
combining keepers' partials), building and checking proposals, the signer, and membership checks. The same code the
native `ome` and `ome-keeper` run.

For integrators; [`@ome-labs/wallet`](https://www.npmjs.com/package/@ome-labs/wallet) builds on it.

## Install

```sh
npm install @ome-labs/wasm
```

The package resolves to a Node build or a browser build through its `node` and `browser` export conditions.

## Use

```ts
import { decodeProposal, verifyAndCombine } from "@ome-labs/wasm";

// Verify each keeper's DLEQ proof and combine t partials into address_secret · P.
const combined = verifyAndCombine(request);

// What a proposal asks for, before reviewing it.
const info = decodeProposal(proposalHex);
```

Types for every input and result (`CombineRequest`, `ProposalInfo`, `VaultInput`, ...) are in the package's
declarations.

## `@ome-labs/wasm/testing`

A separate build with test helpers that create and handle raw secrets (shares, keys). It is for tests only: never
import it in production code.

## Documentation

- [Bindings](https://github.com/ome-labs/ome/blob/main/architecture/bindings.md): what the WebAssembly boundary exposes and why
- [Cryptography](https://github.com/ome-labs/ome/blob/main/architecture/crypto.md) and [protocol](https://github.com/ome-labs/ome/blob/main/architecture/protocol.md)

## Versions

Released together with every other `@ome-labs` package, with one version. Byte-compatible with Aztec 6.0.0.

## License

Apache-2.0
