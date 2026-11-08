//! Aztec's Schnorr signatures over Grumpkin (Barretenberg, Noir `schnorr` v0.4.0): what vault members sign approvals
//! with and what the vault contract verifies.
//!
//! A signature is `(s, e)` with `R = s · G + e · pk` and `e = Poseidon2([DST, R.x, pk.x, pk.y, message])`. On the wire
//! it is `s ‖ e` (32-byte big-endian each) or the four 128-bit limbs `[s.lo, s.hi, e.lo, e.hi]` the contract consumes.

use ark_ff::{PrimeField, Zero};
use ark_grumpkin::Projective;

use crate::{
    Base, Error, Point, Scalar, SecretScalar,
    curve::{base_from_bytes, base_to_bytes, base_to_scalar, scalar_from_bytes, scalar_to_bytes},
    poseidon2,
};

/// `poseidon2_hash_bytes("schnorr_grumpkin_poseidon2")`, the challenge domain separator of Noir `schnorr` v0.4.0.
#[must_use]
pub fn challenge_dst() -> Base {
    Base::from_be_bytes_mod_order(&[
        0x02, 0x4c, 0x76, 0x93, 0x8e, 0xd0, 0x6b, 0x8e, 0xc1, 0xd9, 0x09, 0x4b, 0x10, 0x13, 0xd1,
        0x90, 0xba, 0xa4, 0x01, 0x13, 0x72, 0xf0, 0x60, 0x46, 0x43, 0xbd, 0xa8, 0x12, 0xa6, 0x3b,
        0x83, 0x2e,
    ])
}

/// A Schnorr signature `(s, e)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signature {
    /// Response scalar.
    pub s: Scalar,
    /// Challenge (a Poseidon2 output, below the base field modulus).
    pub e: Base,
}

const LIMB_BYTES: usize = 16;

impl Signature {
    /// Decodes `s ‖ e` (Aztec `SchnorrSignature.toBuffer`).
    ///
    /// # Errors
    /// [`Error::NonCanonicalEncoding`] for components not below their modulus.
    pub fn from_bytes(bytes: &[u8; 64]) -> Result<Self, Error> {
        Ok(Self {
            s: scalar_from_bytes(&bytes[..32])?,
            e: base_from_bytes(&bytes[32..])?,
        })
    }

    /// Encodes `s ‖ e`.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&scalar_to_bytes(&self.s));
        out[32..].copy_from_slice(&base_to_bytes(&self.e));
        out
    }

    /// Decodes the contract's limbs `[s.lo, s.hi, e.lo, e.hi]`.
    ///
    /// # Errors
    /// [`Error::NonCanonicalEncoding`] if a limb is not below `2^128` or a component is not below its modulus.
    pub fn from_limbs(limbs: &[Base; 4]) -> Result<Self, Error> {
        let mut bytes = [0u8; 64];
        for (component, (lo, hi)) in [(&limbs[0], &limbs[1]), (&limbs[2], &limbs[3])]
            .into_iter()
            .enumerate()
        {
            let start = component * 32;
            bytes[start..start + LIMB_BYTES].copy_from_slice(&limb_bytes(hi)?);
            bytes[start + LIMB_BYTES..start + 32].copy_from_slice(&limb_bytes(lo)?);
        }
        Self::from_bytes(&bytes)
    }

    /// Encodes the contract's limbs `[s.lo, s.hi, e.lo, e.hi]` (Aztec `SchnorrSignature.toLimbFields`).
    #[must_use]
    pub fn to_limbs(&self) -> [Base; 4] {
        let bytes = self.to_bytes();
        let limb = |start: usize| {
            let mut padded = [0u8; 32];
            padded[LIMB_BYTES..].copy_from_slice(&bytes[start..start + LIMB_BYTES]);
            Base::from_be_bytes_mod_order(&padded)
        };
        [limb(16), limb(0), limb(48), limb(32)]
    }
}

fn limb_bytes(limb: &Base) -> Result<[u8; LIMB_BYTES], Error> {
    let bytes = base_to_bytes(limb);
    if bytes[..LIMB_BYTES].iter().any(|b| *b != 0) {
        return Err(Error::NonCanonicalEncoding);
    }
    let mut out = [0u8; LIMB_BYTES];
    out.copy_from_slice(&bytes[LIMB_BYTES..]);
    Ok(out)
}

fn challenge(public_key: &Point, r: &Point, message: Base) -> Base {
    poseidon2::hash(&[
        challenge_dst(),
        r.x(),
        public_key.x(),
        public_key.y(),
        message,
    ])
}

/// Verifies a signature on `message` by `public_key` (Noir `schnorr::verify_signature`).
#[must_use]
pub fn verify(public_key: &Point, message: Base, signature: &Signature) -> bool {
    if signature.s.is_zero() || signature.e.is_zero() {
        return false;
    }
    let e = base_to_scalar(&signature.e);
    let r: Projective = Point::generator().mul_public(&signature.s) + public_key.mul_public(&e);
    match Point::try_from(r) {
        Ok(r) => challenge(public_key, &r, message) == signature.e,
        Err(_) => false,
    }
}

/// Signs `message` with `secret_key` as Aztec's TypeScript `Schnorr.constructSignature` does: a fresh random nonce `k`,
/// `R = k · G`, `e = challenge(pk, R, message)`, `s = k - e · sk`. Every signature verifies under [`verify`] and the
/// Noir `schnorr` library the vault contract uses.
///
/// # Errors
/// [`Error::IdentityPoint`] for a zero key or nonce (negligible).
pub fn sign(
    secret_key: &SecretScalar,
    message: Base,
    rng: &mut impl rand_core::CryptoRng,
) -> Result<Signature, Error> {
    let public_key = Point::generator_mul_secret(secret_key)?;
    let nonce = SecretScalar::random(rng);
    let r = Point::generator_mul_secret(&nonce)?;
    let e = challenge(&public_key, &r, message);
    let s = *nonce.expose() - base_to_scalar(&e) * secret_key.expose();
    Ok(Signature { s, e })
}
