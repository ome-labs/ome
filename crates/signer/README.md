# ome-signer

An Ome member's approval signing key: the Grumpkin key whose Schnorr signatures the vault counts toward `k`.

- `SigningKey` is generated on the device and never exported; it cannot be cloned, printed or serialized.
- `SealedKey` is its at-rest form: ChaCha20-Poly1305 under a key derived from a passphrase (Argon2id, `SealingKey`)
  or held by the platform (keychain, keystore), as a small JSON document the caller stores.
- Signing goes only through `approve`, `approve_rotation`, `approve_signer_swap` and `reject`, which run
  `ome-protocol`'s checks first. There is no way to sign an arbitrary hash.

The `Signer` trait leaves room for keys held by platform hardware. The crate does no I/O and builds for WebAssembly
and mobile targets.

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-signer
```

## Learn more

[architecture/signer.md](https://github.com/ome-labs/ome/blob/main/architecture/signer.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
