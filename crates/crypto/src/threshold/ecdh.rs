//! Threshold ECDH: members compute partials of `address_secret · P` with DLEQ proofs; anyone verifies them and
//! combines any `t_v` with Lagrange coefficients.
//!
//! Member `i` holds `x_i = sign · (s_i + preaddress)` (see [`AddressContext::address_secret_share`]) with public
//! `A_i = x_i · G`. For an input point `P` it returns `D_i = x_i · P` and a Chaum-Pedersen proof that
//! `log_G(A_i) = log_P(D_i)`.

use ark_ff::PrimeField;
use ark_grumpkin::Projective;
use rand_core::CryptoRng;
use sha2::{Digest, Sha512};

use crate::{
    Base, Error, Point, Scalar, SecretScalar,
    aztec::AddressContext,
    curve::base_to_bytes,
    threshold::shamir::{MemberIndex, lagrange_coefficient, validate_index_set},
};

/// Domain tag of the DLEQ challenge hash.
pub const DLEQ_DOMAIN: &[u8] = b"ome.dleq.v0";

/// What every partial is bound to: the vault and the committee epoch (the vault config version).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EcdhContext {
    /// Vault address.
    pub vault: Base,
    /// Committee epoch.
    pub epoch: u32,
}

/// Chaum-Pedersen proof in `(challenge, response)` form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DleqProof {
    /// `c = H(transcript)`.
    pub challenge: Scalar,
    /// `z = k + c · x`.
    pub response: Scalar,
}

/// One member's contribution for one input point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partial {
    /// Member index.
    pub index: MemberIndex,
    /// `D_i = x_i · P`.
    pub point: Point,
    /// Proof that `D_i` uses the member's committed share.
    pub proof: DleqProof,
}

/// A member's share of the address secret, ready to answer ECDH requests.
pub struct AddressSecretShare {
    index: MemberIndex,
    share: SecretScalar,
    public: Point,
}

impl AddressSecretShare {
    /// Derives the address-secret share from a viewing share.
    ///
    /// # Errors
    /// [`Error::InvalidIndex`] for index zero; [`Error::IdentityPoint`] for a degenerate share.
    pub fn new(
        index: MemberIndex,
        viewing_share: &SecretScalar,
        context: &AddressContext,
    ) -> Result<Self, Error> {
        if index == 0 {
            return Err(Error::InvalidIndex);
        }
        let share = context.address_secret_share(viewing_share);
        let public = Point::generator_mul_secret(&share)?;
        Ok(Self {
            index,
            share,
            public,
        })
    }

    /// Member index.
    #[must_use]
    pub fn index(&self) -> MemberIndex {
        self.index
    }

    /// `A_i = x_i · G`, equal to [`AddressContext::address_verification_share`] of the member's `Y_i`.
    #[must_use]
    pub fn public(&self) -> &Point {
        &self.public
    }

    /// Computes `D_i = x_i · input` with its DLEQ proof.
    ///
    /// # Errors
    /// [`Error::IdentityPoint`] in degenerate cases (negligible for valid inputs).
    pub fn partial(
        &self,
        input: &Point,
        context: &EcdhContext,
        rng: &mut impl CryptoRng,
    ) -> Result<Partial, Error> {
        let point = input.mul_secret(&self.share)?;
        let nonce = SecretScalar::random(rng);
        let r_generator = Point::generator_mul_secret(&nonce)?;
        let r_input = input.mul_secret(&nonce)?;
        let challenge = challenge(
            context,
            self.index,
            &self.public,
            input,
            &point,
            &r_generator,
            &r_input,
        );
        let response = *nonce.expose() + challenge * self.share.expose();
        Ok(Partial {
            index: self.index,
            point,
            proof: DleqProof {
                challenge,
                response,
            },
        })
    }

    /// Partials for a batch of input points (one proof each).
    ///
    /// # Errors
    /// As [`AddressSecretShare::partial`].
    pub fn partials(
        &self,
        inputs: &[Point],
        context: &EcdhContext,
        rng: &mut impl CryptoRng,
    ) -> Result<Vec<Partial>, Error> {
        inputs
            .iter()
            .map(|input| self.partial(input, context, rng))
            .collect()
    }
}

/// Verifies `partial` for `input` against the member's public address share `A_i`.
///
/// # Errors
/// [`Error::InvalidProof`] if the proof does not verify.
pub fn verify_partial(
    partial: &Partial,
    input: &Point,
    member_public: &Point,
    context: &EcdhContext,
) -> Result<(), Error> {
    let DleqProof {
        challenge: c,
        response: z,
    } = partial.proof;
    // R1 = z·G - c·A, R2 = z·P - c·D
    let r_generator = Point::generator().mul_public(&z) - member_public.mul_public(&c);
    let r_input = input.mul_public(&z) - partial.point.mul_public(&c);
    let (Ok(r_generator), Ok(r_input)) = (Point::try_from(r_generator), Point::try_from(r_input))
    else {
        return Err(Error::InvalidProof(partial.index));
    };
    let expected = challenge(
        context,
        partial.index,
        member_public,
        input,
        &partial.point,
        &r_generator,
        &r_input,
    );
    if expected == c {
        Ok(())
    } else {
        Err(Error::InvalidProof(partial.index))
    }
}

/// Combines verified partials (exactly the chosen set) into `address_secret · P`.
///
/// # Errors
/// [`Error::InsufficientShares`] for fewer than `threshold` partials; index-set errors for zero or duplicate indices.
pub fn combine(partials: &[Partial], threshold: usize) -> Result<Point, Error> {
    if partials.len() < threshold || threshold == 0 {
        return Err(Error::InsufficientShares {
            needed: threshold,
            actual: partials.len(),
        });
    }
    let chosen = &partials[..threshold];
    let set: Vec<MemberIndex> = chosen.iter().map(|p| p.index).collect();
    validate_index_set(&set)?;
    let mut acc = Projective::default();
    for partial in chosen {
        acc += partial
            .point
            .mul_public(&lagrange_coefficient(partial.index, &set)?);
    }
    Point::try_from(acc)
}

/// Verifies every partial against its member's public share (looked up by index) and combines the first `threshold`
/// valid ones. Returns the result and the indices whose partials were invalid.
///
/// # Errors
/// [`Error::InsufficientShares`] if fewer than `threshold` partials verify.
pub fn verify_and_combine(
    partials: &[Partial],
    input: &Point,
    member_public: impl Fn(MemberIndex) -> Option<Point>,
    threshold: usize,
    context: &EcdhContext,
) -> Result<(Point, Vec<MemberIndex>), Error> {
    let mut valid = Vec::with_capacity(threshold);
    let mut invalid = Vec::new();
    for partial in partials {
        let ok = member_public(partial.index)
            .is_some_and(|public| verify_partial(partial, input, &public, context).is_ok())
            && !valid.iter().any(|v: &Partial| v.index == partial.index);
        if ok {
            valid.push(*partial);
        } else {
            invalid.push(partial.index);
        }
    }
    Ok((combine(&valid, threshold)?, invalid))
}

fn challenge(
    context: &EcdhContext,
    index: MemberIndex,
    member_public: &Point,
    input: &Point,
    partial: &Point,
    r_generator: &Point,
    r_input: &Point,
) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(DLEQ_DOMAIN);
    hasher.update(base_to_bytes(&context.vault));
    hasher.update(context.epoch.to_be_bytes());
    hasher.update(index.to_be_bytes());
    for point in [
        &Point::generator(),
        member_public,
        input,
        partial,
        r_generator,
        r_input,
    ] {
        hasher.update(point.to_bytes());
    }
    Scalar::from_le_bytes_mod_order(&hasher.finalize())
}
