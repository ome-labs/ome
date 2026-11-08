//! Proactive resharing to a new committee: same incoming viewing secret, fresh polynomial, new members and threshold.
//!
//! A qualified set `S` of exactly `t` current members (the old threshold) deal. Dealer `i` shares `λ_i^S · s_i` with a
//! fresh polynomial of the new threshold `t'`; its constant-term commitment must equal `λ_i^S · Y_i` (its old public
//! verification share), so a dealer can only contribute its real share. New member `j` sums the sub-shares it
//! receives. The constant terms add up to the unchanged `Ivpk`, so the vault keeps its address, while old and new
//! shares lie on different polynomials and never combine.

use rand_core::CryptoRng;
use sha2::{Digest, Sha512};

use crate::{
    Error, Point, SecretScalar,
    threshold::{
        dkg::KeyShare,
        shamir::{Commitments, MemberIndex, Polynomial, lagrange_coefficient, validate_index_set},
    },
};

/// Domain tag of the reshare transcript hash.
pub const TRANSCRIPT_DOMAIN: &[u8] = b"ome.reshare.transcript.v0";

/// Reshare parameters every participant must agree on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameters {
    /// Unique ceremony id.
    pub session: [u8; 32],
    /// Exactly `old_threshold` current members who deal.
    pub dealers: Vec<MemberIndex>,
    /// Threshold of the current committee.
    pub old_threshold: usize,
    /// Indices of the new committee (its own numbering, `1..=n'`).
    pub new_participants: Vec<MemberIndex>,
    /// Threshold of the new committee.
    pub new_threshold: usize,
}

impl Parameters {
    /// Validates the dealer set and the new committee.
    ///
    /// # Errors
    /// Index-set errors, [`Error::InsufficientShares`] if the dealer set is not exactly the old threshold, or
    /// [`Error::InvalidThreshold`] for a new threshold outside `1..=n'`.
    pub fn validate(&self) -> Result<(), Error> {
        validate_index_set(&self.dealers)?;
        validate_index_set(&self.new_participants)?;
        if self.dealers.len() != self.old_threshold {
            return Err(Error::InsufficientShares {
                needed: self.old_threshold,
                actual: self.dealers.len(),
            });
        }
        if self.new_threshold == 0 || self.new_threshold > self.new_participants.len() {
            return Err(Error::InvalidThreshold);
        }
        Ok(())
    }
}

/// A dealer's broadcast: commitments to its sub-sharing polynomial.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DealerPackage {
    /// Dealer index in the current committee.
    pub dealer: MemberIndex,
    /// Feldman commitments; the constant term must be `λ_dealer · Y_dealer`.
    pub commitments: Commitments,
}

/// A dealer's private sub-share for one new member.
pub struct SubShare {
    /// Dealer index in the current committee.
    pub dealer: MemberIndex,
    /// Recipient index in the new committee.
    pub recipient: MemberIndex,
    /// `g_dealer(recipient)`.
    pub share: SecretScalar,
}

/// Dealer side: sub-shares this member's weighted share to the new committee.
///
/// # Errors
/// Parameter errors, or [`Error::InvalidIndex`] if `current` is not one of the dealers.
pub fn deal(
    parameters: &Parameters,
    current: &KeyShare,
    rng: &mut impl CryptoRng,
) -> Result<(DealerPackage, Vec<SubShare>), Error> {
    parameters.validate()?;
    if !parameters.dealers.contains(&current.index())
        || current.threshold() != parameters.old_threshold
    {
        return Err(Error::InvalidIndex);
    }
    let lambda = lagrange_coefficient(current.index(), &parameters.dealers)?;
    let constant = SecretScalar::new(lambda * current.share().expose());
    let polynomial = Polynomial::random(constant, parameters.new_threshold, rng)?;
    let commitments = polynomial.commitments()?;
    let sub_shares = parameters
        .new_participants
        .iter()
        .map(|&recipient| {
            Ok(SubShare {
                dealer: current.index(),
                recipient,
                share: polynomial.evaluate(recipient)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok((
        DealerPackage {
            dealer: current.index(),
            commitments,
        },
        sub_shares,
    ))
}

/// New-member side: verifies every dealer and sub-share and derives the new key share.
///
/// `old_commitments` are the current committee's group commitments (public, from the committee manifest).
///
/// # Errors
/// [`Error::InvalidProof`] for a dealer whose constant term is not `λ · Y`, [`Error::InvalidShare`] for a sub-share
/// that does not match its dealer's commitments, [`Error::InvalidCommitments`] for wrong commitment lengths, or
/// [`Error::InsufficientShares`] for missing packages.
pub fn receive(
    parameters: &Parameters,
    old_commitments: &Commitments,
    recipient: MemberIndex,
    packages: &[DealerPackage],
    sub_shares: &[SubShare],
) -> Result<KeyShare, Error> {
    parameters.validate()?;
    if !parameters.new_participants.contains(&recipient) {
        return Err(Error::InvalidIndex);
    }
    let missing = || Error::InsufficientShares {
        needed: parameters.dealers.len(),
        actual: packages.len(),
    };

    let mut sum = SecretScalar::new(crate::Scalar::from(0u64));
    let mut ordered = Vec::with_capacity(parameters.dealers.len());
    for &dealer in &parameters.dealers {
        let package = packages
            .iter()
            .find(|p| p.dealer == dealer)
            .ok_or_else(missing)?;
        verify_dealer(parameters, old_commitments, package)?;
        let sub_share = sub_shares
            .iter()
            .find(|s| s.dealer == dealer && s.recipient == recipient)
            .ok_or_else(missing)?;
        if !package
            .commitments
            .verify_share(recipient, &sub_share.share)
        {
            return Err(Error::InvalidShare(dealer));
        }
        sum = SecretScalar::new(*sum.expose() + sub_share.share.expose());
        ordered.push(package);
    }

    let group_commitments = Commitments::sum(ordered.iter().map(|p| &p.commitments))?;
    if group_commitments.constant_term() != old_commitments.constant_term() {
        return Err(Error::GroupKeyMismatch);
    }
    Ok(KeyShare::new(
        recipient,
        parameters.new_threshold,
        parameters.new_participants.clone(),
        sum,
        group_commitments,
        transcript_hash(parameters, &ordered),
    ))
}

/// Checks a dealer package: its polynomial has the new threshold and its constant term is `λ_dealer · Y_dealer`.
///
/// # Errors
/// As [`receive`].
pub fn verify_dealer(
    parameters: &Parameters,
    old_commitments: &Commitments,
    package: &DealerPackage,
) -> Result<(), Error> {
    if package.commitments.threshold() != parameters.new_threshold {
        return Err(Error::InvalidCommitments(package.dealer));
    }
    let lambda = lagrange_coefficient(package.dealer, &parameters.dealers)?;
    let old_share = old_commitments.evaluate(package.dealer)?;
    let expected = Point::try_from(old_share.mul_public(&lambda))?;
    if *package.commitments.constant_term() == expected {
        Ok(())
    } else {
        Err(Error::InvalidProof(package.dealer))
    }
}

fn transcript_hash(parameters: &Parameters, packages: &[&DealerPackage]) -> [u8; 64] {
    let mut hasher = Sha512::new();
    hasher.update(TRANSCRIPT_DOMAIN);
    hasher.update(parameters.session);
    hasher.update((parameters.new_threshold as u64).to_be_bytes());
    for index in &parameters.new_participants {
        hasher.update(index.to_be_bytes());
    }
    for package in packages {
        hasher.update(package.dealer.to_be_bytes());
        for point in package.commitments.points() {
            hasher.update(point.to_bytes());
        }
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(&hasher.finalize());
    out
}
