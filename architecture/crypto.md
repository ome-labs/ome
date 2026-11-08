# ome-crypto

`crates/crypto` holds every cryptographic primitive Ome uses. It is pure: no I/O and no clock, and randomness comes
only from `rand_core::CryptoRng` parameters. It builds for native targets and `wasm32-unknown-unknown`
(`scripts/wasm-core.sh`). It is not `no_std`, because `taceo-poseidon2` requires `std`. On wasm it enables
`getrandom` 0.2's `js` backend, which is the `getrandom` version arkworks uses.

## Modules

| Module               | What it provides                                                                                                                |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `curve`              | Grumpkin `Point`, `Base` (BN254 Fr) and `Scalar` (BN254 Fq); Aztec's 64-byte and compressed encodings; canonical field decoding |
| `curve::ladder`      | constant-time scalar multiplication for secret scalars (below)                                                                  |
| `secret`             | `SecretScalar`: zeroized on drop, not `Clone`, redacted `Debug`, constant-time equality                                         |
| `poseidon2`          | Aztec's Poseidon2 permutation, sponge hash and hash with a domain separator                                                     |
| `aztec`              | `preaddress`, `AddressContext` (address, address point, address-secret sign), address-secret shares, app-siloed ECDH secret     |
| `schnorr`            | Aztec's Grumpkin Schnorr signatures: sign, verify, 64-byte and 4-limb encodings                                                 |
| `threshold::shamir`  | polynomials, Feldman commitments, Lagrange coefficients at zero, commitment interpolation from verification shares              |
| `threshold::dkg`     | Feldman DKG: round 1 (commitments and a proof of knowledge), round 2 (private shares), finalize, complaints                     |
| `threshold::ecdh`    | threshold ECDH partials with Chaum-Pedersen DLEQ proofs; verify and combine                                                     |
| `threshold::reshare` | resharing to a new committee and threshold with the group key unchanged                                                         |
| `sealing::envelope`  | HPKE envelopes (X25519, HKDF-SHA256, ChaCha20-Poly1305, auth mode) bound to a session, epoch, kind and both members             |
| `sealing::mailbox`   | per-pair, per-day mailbox ids and payload sealing for store-and-forward delivery                                                |

## Threshold viewing

The vault's address is `x(preaddress · G + Ivpk)`. Aztec's address secret is `σ · (ivsk + preaddress)`, where the
sign `σ` is `-1` when the address point has a "negative" `y`. The sign depends only on public data. Each member maps
its viewing share `s_i` to `σ · (s_i + preaddress)`. Lagrange weights at zero sum to one, so these values are shares
of the address secret. No member ever learns `ivsk`.

- **DKG.** Each dealer commits to a random polynomial of degree `t - 1` and proves knowledge of its constant term with
  a Schnorr proof bound to the session and the dealer's index. Recipients check their shares against the dealer's
  commitments. The group key is the sum of the constant-term commitments. Every failure names the dealer:
  `InvalidProof(d)`, `InvalidCommitments(d)` (the polynomial has the wrong degree) or `InvalidShare(d)`.
  `complaint_is_valid` lets the others check a revealed share against the dealer's commitments.
- **Threshold ECDH.** A partial is `a_i · P`, where `a_i` is the member's address-secret share. It carries a DLEQ
  proof that it uses the same `a_i` as the member's public share `a_i · G`. The proof is bound to the member index,
  the vault address, the committee epoch and `P`. `verify_and_combine` drops invalid partials, names them, and
  combines the rest only if at least `t` remain.
- **Resharing.** `t` old members deal `g_i` with `g_i(0) = λ_i · s_i`. Recipients check `g_i(0) · G = λ_i · Y_i`
  against the old group commitments, then sum their sub-shares. The group key and every decryption output stay the
  same. Shares from the old committee do not combine with shares from the new one.
- `Commitments::interpolate` rebuilds group commitments from the published verification shares. `ome-protocol` uses
  it to check that a rotation's committee interpolates to `Ivpk`.

## Domain tags

Every tag ends in `v0` and changes only together with every encoder.

| Tag                         | Used for                                   |
| --------------------------- | ------------------------------------------ |
| `ome.dkg.pok.v0`            | DKG proof-of-knowledge challenge (SHA-512) |
| `ome.dkg.transcript.v0`     | DKG transcript hash                        |
| `ome.dleq.v0`               | threshold ECDH DLEQ challenge              |
| `ome.reshare.transcript.v0` | reshare transcript hash                    |
| `ome.envelope.v0`           | HPKE `info` prefix                         |
| `ome.mailbox.v0`            | mailbox id derivation (SHA-256)            |

## Compatibility with Aztec

`tests/aztec_compat.rs` and `tests/schnorr.rs` check this crate against `tests/vectors/aztec.json`. That file is
generated by `tools/vectors` from Aztec 6.0.0's own packages. The vectors cover:

- point encodings;
- public and constant-time scalar multiplication, including edge scalars;
- the Poseidon2 permutation, plain hashes and hashes with a domain separator;
- account derivation: `ivpk`, preaddress, address, address point, and the address secret with both signs;
- ECDH and the app-siloed shared secret;
- Schnorr signatures and their limb encoding.

AES key derivation and log tags are not covered: Aztec 6.0.0 has no TypeScript implementation to generate them from.
They are left to the end-to-end tests against a local network.

## Constant time

Secret scalars are multiplied only through `curve::ladder`. arkworks' scalar multiplication branches on scalar bits,
so the ladder does not use it. Instead it uses:

- complete projective addition for `a = 0` curves (Renes, Costello and Batina 2016, Algorithm 7);
- a fixed 4-bit window over all 256 bits;
- constant-time table selection with `subtle`;
- inversion by Fermat's little theorem.

The remaining known timing variation is arkworks' Montgomery multiplication, whose final subtraction is conditional.
`tests/properties.rs` checks that the ladder agrees with arkworks for random scalars and points.

## Dependencies

All dependencies are pinned to their latest releases. arkworks 0.6 still depends on `digest` 0.10 and `rand_core`
0.6, so `cargo deny` reports these as duplicates of the newer versions Ome uses directly. This is a warning, not an
error.
