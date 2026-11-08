# @ome-labs/vectors

Golden vectors computed with Aztec's own packages (`@aztec-labs/*`, pinned in the workspace catalog) for Ome's
byte-compatibility tests. Values come from Aztec, never from Ome code, so a Rust test that matches them proves
compatibility rather than self-consistency.

```sh
pnpm --filter @ome-labs/vectors generate   # rewrite the vector files from the installed Aztec
pnpm --filter @ome-labs/vectors test       # fail if a committed file differs from what Aztec produces
```

| File                                          | Contents                                                                                                                                                                                    | Used by      |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| `crates/crypto/tests/vectors/aztec.json`      | field moduli, Grumpkin generator and scalar multiplication, Poseidon2, Schnorr signatures, accounts (key derivation, preaddress, address, address secret for both signs, ECDH, app siloing) | ome-crypto   |
| `crates/protocol/tests/vectors/contract.json` | `ome.*.v0` signer-set and committee commitments and approval messages, equal to the Noir contract's own vectors                                                                             | ome-protocol |

Every file starts with `source` (this package and version) and `aztecVersion`. Run `generate` and the Rust tests on
every Aztec upgrade: a changed vector means Aztec changed something Ome depends on.

Schnorr signing uses a random nonce, so committed signatures are kept while they still verify under the installed
Aztec; regenerating does not rewrite them.
