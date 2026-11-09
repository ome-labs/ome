# ome-protocol

The rules of an Ome vault in one place, so every client decides the same way: the keeper, the daemon (through
`@ome-labs/wasm`), a phone approving on its own, and future apps.

## Modules

- `vault` mirrors the `OmeVault` contract: signer-set and committee commitments, config transitions, the approval
  message and the 49-field auth witness.
- `payload` recomputes the hash a vault transaction is authorized under from its calls, so a member checks what they
  sign instead of trusting the proposer.
- `proposals` checks proposals against their payload, verifies approvals and rejections, and tallies them.
- `membership` checks a manifest against the on-chain config, checks rotations, and warns about risky policies.
- `lobby` holds the founding rules: join proofs, sealed founding secrets and the vault's protocol version.

Pure: no I/O and no clock; callers pass the time. The encodings it shares with the Noir contract are checked against
the contract's own test vectors.

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-protocol
```

## Learn more

[architecture/protocol.md](https://github.com/ome-labs/ome/blob/main/architecture/protocol.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
