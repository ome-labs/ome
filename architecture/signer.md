# ome-signer

`crates/signer` holds a member's **approval signing key**: the Grumpkin key whose Schnorr signatures `OmeVault`
counts toward `k`. It builds on `ome-crypto` (Schnorr), `ome-protocol` (the rules it enforces before signing) and
`ome-wire` (the approval and rejection messages). It does no I/O and builds for `wasm32-unknown-unknown`.

The keeper's Ed25519 identity and the viewing shares are not handled here: they belong to `ome-wire` and
`ome-keeper`.

## Key lifecycle

1. **Generate.** `SigningKey::generate(rng)` runs on the member's device. Its public key goes into the vault's signer
   set.
2. **Seal.** `SealedKey::seal(key, secret, rng)` produces a small JSON document. The caller stores it wherever the
   device keeps secrets:
   - the daemon: a file;
   - Apple platforms: Keychain;
   - Android: Keystore;
   - browser: IndexedDB.
3. **Open to sign.** `SealedKey::open(secret)` returns the `SigningKey`. The caller decides how long to keep it open,
   for example an unlock window in the daemon. The crate has no clock.
4. **Re-seal.** `SealedKey::reseal(current, next, rng)` opens the key with the current secret and seals the same key
   under the new passphrase, with a fresh salt and nonce. The public key is unchanged; the sealing key is not, so a
   platform that kept it fetches it again.
5. **Never export.** `SigningKey` has no `Clone`, `Debug` or `Serialize` and no accessor for the scalar. The only way
   out of memory is sealing.

The secret that unlocks a sealed key is a `Secret`:

- **`Passphrase`**: the key is derived with Argon2id.
- **`Key(SealingKey)`**: a 32-byte key, used in one of two ways:
  - a platform-held key the signing key was sealed under from the start (`"alg": "external"`);
  - the key a passphrase derives, which `SealedKey::sealing_key(passphrase)` returns after checking it opens the
    file. A platform keeps that behind biometrics and later opens the file without running Argon2id.

`SealingKey` is zeroized on drop and has no `Clone` or `Debug`.

## Sealed format

```json
{
  "v": "ome.signer.v0",
  "publicKey": { "x": "0x…", "y": "0x…" },
  "kdf": { "alg": "argon2id", "salt": "…16 bytes…", "memoryKib": 65536, "passes": 3, "lanes": 1 },
  "nonce": "…12 bytes…",
  "ciphertext": "…48 bytes…"
}
```

- **Cipher.** ChaCha20-Poly1305 over the 32-byte big-endian scalar.
- **Associated data.** `"ome.signer.v0" ‖ kdf (alg, salt, parameters) ‖ public key`. Editing the parameters, the
  salt or the public key makes the file fail to open. On open, the decrypted scalar must also reproduce the stored
  public key.
- **KDF parameters.** New keys use Argon2id with 64 MiB, 3 passes and 1 lane, a 16-byte salt and a 32-byte output.
  The parameters are stored in the file so they can be raised later. `from_json` and `open` refuse:
  - anything below 64 MiB or 3 passes, so an edited file can't make brute force cheaper;
  - anything above 1 GiB, 16 passes or 8 lanes, so a crafted file can't hang a device.
- **Format checks.** Unknown versions, unknown fields, unknown algorithms and documents over 4 KiB are refused. Hex
  must be lowercase and of exact length.

Measured cost of one open with the default parameters, on Apple silicon: 67 ms native (release) and 95 ms as
`wasm32-wasip1` under Node 24. 64 MiB stays usable in a browser. Debug and test builds compile `argon2` and `blake2`
with optimizations.

## What the signer signs

There is no `sign(hash)`. A `Signer` signs a `Message`, and only this crate can create one. The public entry points
are:

| Function              | Checks before signing                                                                                                                                                                                                                                                                      | Returns     |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------- |
| `approve`             | `check_proposal` (payload matches, calls hash to `outer_hash` and authwits to `authwits`, current config, before the deadline, proposer is a member, external fees); no call is `rotate_membership` on the vault. Signs the transaction and every authwit                                  | `Approval`  |
| `approve_rotation`    | `check_proposal`; the payload has exactly one `rotate_membership` call, every other call targets a fee payer the member trusts, and it grants no authwit; the rotation's decoded arguments equal the reviewed one; `check_rotation` (contract rules, shares hold `Ivpk`, shares refreshed) | `Approval`  |
| `approve_signer_swap` | `check_proposal`; exactly one `rotate_membership` call plus trusted fee payer calls, as for `approve_rotation`; `check_signer_swap` (one key replaced, committee and thresholds unchanged, contract rules). Returns the swapped slot too                                                   | `Approval`  |
| `reject`              | the reason fits the wire limit                                                                                                                                                                                                                                                             | `Rejection` |

Every entry point derives the slot from the signer's own public key. A key outside the signer set gets `NotASigner`.

A compromised proposer or front end therefore can't get a member's signature on:

- a different transaction than the payload describes;
- a stale or expired proposal;
- a proposal that pays fees from the vault in public;
- a rotation that leaves the vault unreadable or keeps removed members' shares valid;
- a rotation that also moves funds, or grants an authwit;
- an authwit other than the ones the payload describes, or one granted to a contract the transaction doesn't call;
- a signer swap that changes anything but one key.

**Fee payers in a rotation.** A sponsored fee payer is paid through a call in the vault's own payload, so a rotation
proposal carries one. `approve_rotation` therefore takes `fee_payers`, the fee payer contracts the member's own
configuration trusts, never a list the proposer supplies. Every call besides the rotation must target one of them.

The `rotate_membership` selector (`0xe1071b7c`) and its 67-field argument layout (current keys, new keys, count, `k`,
`t_v`, shares, committee polynomial) are decoded by `ome-protocol` (`Rotation::from_call`, which also requires the
polynomial to be the shares' own) and pinned by the contract's `call_layout` test.

## Hardware keys

`Signer` is a trait so that a Secure Enclave or Android Keystore key can be added later without changing callers.
Those keys are P-256, which the contract can't verify today; adding them needs a contract change that accepts mixed
signer schemes. Only the software Grumpkin signer (`SigningKey`) exists now.

## Tests

- `tests/sealing.rs`:
  - passphrase-sealed, derived-key and platform-sealed keys open;
  - a wrong secret, the wrong kind of secret, a flipped ciphertext digit, a swapped public key and edited parameters
    are refused;
  - weak, absurd, unknown and oversized files are refused before opening.
- `tests/sign.rs`:
  - approvals verify and reach the threshold;
  - an approval doesn't carry over to another proposal;
  - a mismatched payload, an expired proposal and a public fee are never signed;
  - a key outside the signer set can't vote;
  - rejections verify, and their reason is bounded.
- `tests/rotation.rs`:
  - a reviewed reshare is approved;
  - a rotation can't be approved as an ordinary proposal, or bundled with another call;
  - it may also call a trusted fee payer, but not an untrusted one, and never carry two rotations;
  - only the reviewed rotation is signed;
  - a rotation that keeps the old shares is refused;
  - a signer swap is approved only through `approve_signer_swap`, and a reshare is not a swap.
