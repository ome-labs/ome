# ome-crypto

The cryptography behind Ome's threshold viewing, byte-compatible with Aztec 6.0.0. An Ome vault's incoming viewing
secret is never assembled: members' keepers hold Shamir shares of it, and the wallet decrypts notes through threshold
ECDH. This crate holds every primitive that makes that work.

## Modules

- `curve`, `poseidon2`: Grumpkin points and scalars with Aztec's encodings, and Aztec's Poseidon2.
- `aztec`: preaddress, address context, the address-secret sign rule and app siloing.
- `schnorr`: Aztec's Grumpkin Schnorr signatures, the scheme members approve with.
- `threshold`: Shamir sharing with Feldman commitments, distributed key generation, threshold ECDH partials with DLEQ
  proofs, and resharing to a new committee.
- `sealing`: authenticated envelopes and relay mailboxes.

Secret scalars are `SecretScalar`: zeroized on drop, never printed or cloned, and multiplied with a constant-time
ladder. Randomness is always a caller-provided `rand_core::CryptoRng`, so the crate runs unchanged natively, in
WebAssembly (`ome-wasm`) and on phones (`ome-ffi`). It does no I/O.

Compatibility with Aztec (point encoding, address-secret derivation, ECDH-to-AES, tags) is proven by golden vectors
generated from the real Aztec 6.0.0 packages (`tools/vectors`).

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-crypto
```

## Learn more

[architecture/crypto.md](https://github.com/ome-labs/ome/blob/main/architecture/crypto.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
