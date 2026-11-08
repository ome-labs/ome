//! Distributed key generation for the incoming viewing secret (Pedersen / Feldman with proofs of knowledge, the
//! FROST `KeyGen` shape). Nobody ever holds the secret; each member ends with a share `s_i`, and everyone learns the
//! group key `Ivpk` and every verification share `Y_i = s_i · G`.
//!
//! The functions are a pure state machine: callers move [`Round1Package`]s by reliable broadcast and
//! [`Round2Package`]s privately (sealed with [`crate::sealing::envelope`] and signed by the dealer, so complaints are
//! attributable).

use ark_ff::PrimeField;
use rand_core::CryptoRng;
use sha2::{Digest, Sha512};

use crate::{
    Error, Point, Scalar, SecretScalar,
    threshold::shamir::{Commitments, MemberIndex, Polynomial, validate_index_set},
};

/// Domain tag of the proof-of-knowledge challenge.
pub const POK_DOMAIN: &[u8] = b"ome.dkg.pok.v0";
/// Domain tag of the transcript hash.
pub const TRANSCRIPT_DOMAIN: &[u8] = b"ome.dkg.transcript.v0";

/// Ceremony parameters every participant must agree on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameters {
    /// Unique ceremony id (agreed out of band, e.g. random from the initiator); binds proofs to this ceremony.
    pub session: [u8; 32],
    /// Reconstruction threshold `t_v`.
    pub threshold: usize,
    /// Participant indices.
    pub participants: Vec<MemberIndex>,
}

impl Parameters {
    /// Validates the parameters: distinct non-zero indices and `1 <= threshold <= n`.
    ///
    /// # Errors
    /// Index-set errors or [`Error::InvalidThreshold`].
    pub fn validate(&self) -> Result<(), Error> {
        validate_index_set(&self.participants)?;
        if self.threshold == 0 || self.threshold > self.participants.len() {
            return Err(Error::InvalidThreshold);
        }
        Ok(())
    }

    fn require_participant(&self, index: MemberIndex) -> Result<(), Error> {
        if self.participants.contains(&index) {
            Ok(())
        } else {
            Err(Error::InvalidIndex)
        }
    }
}

/// Schnorr proof of knowledge of a commitment's discrete log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProofOfKnowledge {
    /// `R = k · G`.
    pub commitment: Point,
    /// `mu = k + c · a`.
    pub response: Scalar,
}

/// Broadcast message of round 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Round1Package {
    /// Dealer index.
    pub dealer: MemberIndex,
    /// Feldman commitments to the dealer's polynomial.
    pub commitments: Commitments,
    /// Proof of knowledge of the polynomial's constant term.
    pub proof: ProofOfKnowledge,
}

/// A dealer's private state between rounds.
pub struct Round1Secret {
    index: MemberIndex,
    polynomial: Polynomial,
}

/// Private message of round 2: the dealer's polynomial evaluated at the recipient.
pub struct Round2Package {
    /// Dealer index.
    pub dealer: MemberIndex,
    /// Recipient index.
    pub recipient: MemberIndex,
    /// `f_dealer(recipient)`.
    pub share: SecretScalar,
}

/// A member's result: its secret share and the public committee data.
pub struct KeyShare {
    index: MemberIndex,
    threshold: usize,
    participants: Vec<MemberIndex>,
    share: SecretScalar,
    group_commitments: Commitments,
    transcript: [u8; 64],
}

impl KeyShare {
    pub(crate) fn new(
        index: MemberIndex,
        threshold: usize,
        participants: Vec<MemberIndex>,
        share: SecretScalar,
        group_commitments: Commitments,
        transcript: [u8; 64],
    ) -> Self {
        Self {
            index,
            threshold,
            participants,
            share,
            group_commitments,
            transcript,
        }
    }

    /// Restores a stored key share, checking that `share · G` is this member's committed verification share.
    ///
    /// # Errors
    /// [`Error::InvalidIndex`] if `index` is not a participant, [`Error::InvalidThreshold`] if the commitments do not
    /// have `threshold` terms, [`Error::InvalidShare`] if the share does not match.
    pub fn restore(
        index: MemberIndex,
        threshold: usize,
        participants: Vec<MemberIndex>,
        share: SecretScalar,
        group_commitments: Commitments,
        transcript: [u8; 64],
    ) -> Result<Self, Error> {
        validate_index_set(&participants)?;
        if !participants.contains(&index) {
            return Err(Error::InvalidIndex);
        }
        if group_commitments.threshold() != threshold || threshold > participants.len() {
            return Err(Error::InvalidThreshold);
        }
        if Point::generator_mul_secret(&share)? != group_commitments.evaluate(index)? {
            return Err(Error::InvalidShare(index));
        }
        Ok(Self::new(
            index,
            threshold,
            participants,
            share,
            group_commitments,
            transcript,
        ))
    }

    /// This member's index.
    #[must_use]
    pub fn index(&self) -> MemberIndex {
        self.index
    }

    /// Viewing threshold `t_v`.
    #[must_use]
    pub fn threshold(&self) -> usize {
        self.threshold
    }

    /// Committee members.
    #[must_use]
    pub fn participants(&self) -> &[MemberIndex] {
        &self.participants
    }

    /// The secret viewing share `s_i`.
    #[must_use]
    pub fn share(&self) -> &SecretScalar {
        &self.share
    }

    /// The group incoming viewing public key `Ivpk`.
    #[must_use]
    pub fn group_public_key(&self) -> &Point {
        self.group_commitments.constant_term()
    }

    /// Commitments to the shared polynomial; `evaluate(i)` is member `i`'s verification share.
    #[must_use]
    pub fn group_commitments(&self) -> &Commitments {
        &self.group_commitments
    }

    /// `Y_j = s_j · G` for member `j`.
    ///
    /// # Errors
    /// [`Error::InvalidIndex`] for a non-member.
    pub fn verification_share(&self, member: MemberIndex) -> Result<Point, Error> {
        if !self.participants.contains(&member) {
            return Err(Error::InvalidIndex);
        }
        self.group_commitments.evaluate(member)
    }

    /// Verification shares of all members, in participant order.
    ///
    /// # Errors
    /// As [`KeyShare::verification_share`].
    pub fn verification_shares(&self) -> Result<Vec<Point>, Error> {
        self.participants
            .iter()
            .map(|&j| self.verification_share(j))
            .collect()
    }

    /// Hash of the ceremony transcript; members confirm they all hold the same one.
    #[must_use]
    pub fn transcript(&self) -> &[u8; 64] {
        &self.transcript
    }
}

/// Round 1: samples this member's polynomial and builds the broadcast package.
///
/// # Errors
/// Parameter errors, or [`Error::InvalidIndex`] if `index` is not a participant.
pub fn round1(
    parameters: &Parameters,
    index: MemberIndex,
    rng: &mut impl CryptoRng,
) -> Result<(Round1Secret, Round1Package), Error> {
    parameters.validate()?;
    parameters.require_participant(index)?;
    let constant = SecretScalar::random(rng);
    let polynomial = Polynomial::random(constant, parameters.threshold, rng)?;
    let commitments = polynomial.commitments()?;
    let proof = prove_knowledge(parameters, index, &polynomial, &commitments, rng)?;
    Ok((
        Round1Secret { index, polynomial },
        Round1Package {
            dealer: index,
            commitments,
            proof,
        },
    ))
}

/// Round 2: verifies every other dealer's round-1 package and returns this member's shares for them.
///
/// # Errors
/// [`Error::InvalidProof`] / [`Error::InvalidCommitments`] naming the offending dealer's package, or
/// [`Error::InsufficientShares`] if packages are missing.
pub fn round2(
    parameters: &Parameters,
    secret: &Round1Secret,
    round1: &[Round1Package],
) -> Result<Vec<Round2Package>, Error> {
    verify_round1(parameters, round1)?;
    parameters
        .participants
        .iter()
        .filter(|&&recipient| recipient != secret.index)
        .map(|&recipient| {
            Ok(Round2Package {
                dealer: secret.index,
                recipient,
                share: secret.polynomial.evaluate(recipient)?,
            })
        })
        .collect()
}

/// Finalisation: checks every received share against its dealer's commitments and derives this member's key share.
///
/// # Errors
/// [`Error::InvalidShare`] naming a dealer whose share does not match its commitments (raise a complaint), or package
/// errors as in [`round2`].
pub fn finalize(
    parameters: &Parameters,
    secret: Round1Secret,
    round1: &[Round1Package],
    received: &[Round2Package],
) -> Result<KeyShare, Error> {
    verify_round1(parameters, round1)?;
    let me = secret.index;
    let mut sum = *secret.polynomial.evaluate(me)?.expose();
    for package in round1.iter().filter(|p| p.dealer != me) {
        let share = received
            .iter()
            .find(|r| r.dealer == package.dealer && r.recipient == me)
            .ok_or(Error::InsufficientShares {
                needed: parameters.participants.len(),
                actual: received.len() + 1,
            })?;
        if !package.commitments.verify_share(me, &share.share) {
            return Err(Error::InvalidShare(package.dealer));
        }
        sum += share.share.expose();
    }
    // The secret polynomial must not outlive the ceremony: consume it (coefficients zeroize on drop).
    drop(secret);
    let group_commitments = Commitments::sum(round1.iter().map(|p| &p.commitments))?;
    let share = SecretScalar::new(sum);
    if Point::generator_mul_secret(&share)? != group_commitments.evaluate(me)? {
        return Err(Error::InvalidShare(me));
    }
    Ok(KeyShare::new(
        me,
        parameters.threshold,
        parameters.participants.clone(),
        share,
        group_commitments,
        transcript_hash(parameters, round1),
    ))
}

/// Verifies a complaint: `true` if the revealed share does **not** match the dealer's commitments, i.e. the dealer
/// misbehaved.
#[must_use]
pub fn complaint_is_valid(
    dealer_package: &Round1Package,
    recipient: MemberIndex,
    revealed_share: &SecretScalar,
) -> bool {
    !dealer_package
        .commitments
        .verify_share(recipient, revealed_share)
}

fn verify_round1(parameters: &Parameters, round1: &[Round1Package]) -> Result<(), Error> {
    parameters.validate()?;
    if round1.len() != parameters.participants.len() {
        return Err(Error::InsufficientShares {
            needed: parameters.participants.len(),
            actual: round1.len(),
        });
    }
    for &participant in &parameters.participants {
        let package =
            round1
                .iter()
                .find(|p| p.dealer == participant)
                .ok_or(Error::InsufficientShares {
                    needed: parameters.participants.len(),
                    actual: round1.len(),
                })?;
        if package.commitments.threshold() != parameters.threshold {
            return Err(Error::InvalidCommitments(package.dealer));
        }
        verify_knowledge(parameters, package)?;
    }
    Ok(())
}

fn prove_knowledge(
    parameters: &Parameters,
    index: MemberIndex,
    polynomial: &Polynomial,
    commitments: &Commitments,
    rng: &mut impl CryptoRng,
) -> Result<ProofOfKnowledge, Error> {
    let nonce = SecretScalar::random(rng);
    let commitment = Point::generator_mul_secret(&nonce)?;
    let challenge = pok_challenge(parameters, index, commitments.constant_term(), &commitment);
    let constant = polynomial.constant();
    Ok(ProofOfKnowledge {
        commitment,
        response: *nonce.expose() + challenge * constant.expose(),
    })
}

fn verify_knowledge(parameters: &Parameters, package: &Round1Package) -> Result<(), Error> {
    let c = pok_challenge(
        parameters,
        package.dealer,
        package.commitments.constant_term(),
        &package.proof.commitment,
    );
    // mu · G == R + c · C_0
    let lhs = Point::generator().mul_public(&package.proof.response);
    let rhs =
        package.commitments.constant_term().mul_public(&c) + package.proof.commitment.as_affine();
    if lhs == rhs {
        Ok(())
    } else {
        Err(Error::InvalidProof(package.dealer))
    }
}

fn pok_challenge(
    parameters: &Parameters,
    index: MemberIndex,
    constant: &Point,
    commitment: &Point,
) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(POK_DOMAIN);
    hasher.update(parameters.session);
    hasher.update(index.to_be_bytes());
    hasher.update(constant.to_bytes());
    hasher.update(commitment.to_bytes());
    Scalar::from_le_bytes_mod_order(&hasher.finalize())
}

fn transcript_hash(parameters: &Parameters, round1: &[Round1Package]) -> [u8; 64] {
    let mut hasher = Sha512::new();
    hasher.update(TRANSCRIPT_DOMAIN);
    hasher.update(parameters.session);
    hasher.update((parameters.threshold as u64).to_be_bytes());
    for &participant in &parameters.participants {
        hasher.update(participant.to_be_bytes());
        if let Some(package) = round1.iter().find(|p| p.dealer == participant) {
            for point in package.commitments.points() {
                hasher.update(point.to_bytes());
            }
        }
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(&hasher.finalize());
    out
}
