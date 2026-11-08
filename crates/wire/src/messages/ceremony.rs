//! Committee manifests and the DKG and reshare ceremonies.

use ome_crypto::{
    Point, SecretScalar,
    aztec::AddressContext,
    sealing::envelope::Envelope,
    threshold::{
        dkg::{ProofOfKnowledge, Round1Package},
        reshare::DealerPackage,
        shamir::{Commitments, MemberIndex},
    },
};
use serde::{Deserialize, Serialize};

use super::bounded;
use crate::{
    Error, IdentityPublic,
    limits::{
        MAX_ADDRESS_BYTES, MAX_ADDRESSES, MAX_ECHO_BYTES, MAX_MEMBERS, MAX_SEALED_SHARE_BYTES,
    },
    types::{
        Bytes64, WireField, WirePoint, WireScalar, field, parse_field, parse_points, parse_scalar,
        scalar,
    },
    version,
};

/// The committee as every member's keeper knows it. Checked against the vault's on-chain commitments before use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Protocol version the vault's members agreed on.
    pub protocol_version: u16,
    /// Vault address.
    pub vault: WireField,
    /// Committee epoch: the vault config version at which this committee became active.
    pub epoch: u32,
    /// Spending threshold `k`.
    pub threshold: u32,
    /// Viewing threshold `t_v`.
    pub viewing_threshold: u32,
    /// Vault preaddress.
    pub preaddress: WireField,
    /// Group incoming viewing key `Ivpk`.
    pub group_key: WirePoint,
    /// Members in slot order; member `i` (1-based) is signer slot `i - 1`.
    pub members: Vec<Member>,
}

/// One committee member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// 1-based member index.
    pub index: MemberIndex,
    /// Grumpkin key the member signs approvals with.
    pub signer_key: WirePoint,
    /// Viewing verification share `Y_i`.
    pub verification_share: WirePoint,
    /// The member's keeper identity.
    pub identity: IdentityPublic,
    /// The keeper's X25519 transport key.
    pub transport: [u8; 32],
    /// Where to reach the keeper.
    pub addresses: Vec<String>,
}

impl Manifest {
    /// Structural checks: a supported protocol version, 1..=8 members indexed `1..=n` in order, distinct identities,
    /// `1 <= k <= n`, `2 <= t_v <= n`, bounded addresses.
    ///
    /// # Errors
    /// [`Error::UnsupportedVersion`] or [`Error::OutOfBounds`] naming the field.
    pub fn validate(&self) -> Result<(), Error> {
        version::require_supported(self.protocol_version)?;
        bounded("members", self.members.len(), 1, MAX_MEMBERS)?;
        let n = u32::try_from(self.members.len())
            .map_err(|_| Error::OutOfBounds { field: "members" })?;
        if !(1..=n).contains(&self.threshold) {
            return Err(Error::OutOfBounds { field: "threshold" });
        }
        if !(2..=n).contains(&self.viewing_threshold) {
            return Err(Error::OutOfBounds {
                field: "viewing_threshold",
            });
        }
        for (position, member) in self.members.iter().enumerate() {
            if usize::try_from(member.index).ok() != Some(position + 1) {
                return Err(Error::OutOfBounds {
                    field: "members.index",
                });
            }
            if self.members[..position]
                .iter()
                .any(|m| m.identity == member.identity)
            {
                return Err(Error::OutOfBounds {
                    field: "members.identity",
                });
            }
            addresses(&member.addresses)?;
        }
        Ok(())
    }

    /// Signer keys in slot order.
    ///
    /// # Errors
    /// [`Error::Crypto`] for an invalid point.
    pub fn signer_keys(&self) -> Result<Vec<Point>, Error> {
        self.members
            .iter()
            .map(|m| Point::try_from(&m.signer_key))
            .collect()
    }

    /// Verification shares in member order.
    ///
    /// # Errors
    /// [`Error::Crypto`] for an invalid point.
    pub fn verification_shares(&self) -> Result<Vec<Point>, Error> {
        self.members
            .iter()
            .map(|m| Point::try_from(&m.verification_share))
            .collect()
    }

    /// The address context, after checking that the vault address follows from `preaddress` and `group_key`.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] (`vault`) on a mismatch, or point errors.
    pub fn address_context(&self) -> Result<AddressContext, Error> {
        let context = AddressContext::new(
            parse_field(&self.preaddress)?,
            &Point::try_from(&self.group_key)?,
        )?;
        if field(&context.address()) != self.vault {
            return Err(Error::OutOfBounds { field: "vault" });
        }
        Ok(context)
    }

    /// The member at `index`.
    #[must_use]
    pub fn member(&self, index: MemberIndex) -> Option<&Member> {
        self.members.iter().find(|m| m.index == index)
    }
}

/// At most [`MAX_ADDRESSES`] addresses of at most [`MAX_ADDRESS_BYTES`] each.
pub(crate) fn addresses(list: &[String]) -> Result<(), Error> {
    bounded("addresses", list.len(), 0, MAX_ADDRESSES)?;
    if list.iter().any(|a| a.len() > MAX_ADDRESS_BYTES) {
        return Err(Error::OutOfBounds { field: "addresses" });
    }
    Ok(())
}

/// Which ceremony a complaint, justification or abort refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ceremony {
    /// Distributed key generation.
    Dkg,
    /// Committee resharing.
    Reshare,
}

/// DKG round 1 broadcast.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DkgRound1 {
    /// Dealer index.
    pub dealer: MemberIndex,
    /// Feldman commitments, constant term first.
    pub commitments: Vec<WirePoint>,
    /// Proof-of-knowledge commitment `R`.
    pub proof_commitment: WirePoint,
    /// Proof-of-knowledge response `mu`.
    pub proof_response: WireScalar,
}

impl DkgRound1 {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("commitments", self.commitments.len(), 1, MAX_MEMBERS)
    }
}

impl From<&Round1Package> for DkgRound1 {
    fn from(package: &Round1Package) -> Self {
        Self {
            dealer: package.dealer,
            commitments: package
                .commitments
                .points()
                .iter()
                .map(WirePoint::from)
                .collect(),
            proof_commitment: WirePoint::from(&package.proof.commitment),
            proof_response: scalar(&package.proof.response),
        }
    }
}

impl TryFrom<&DkgRound1> for Round1Package {
    type Error = Error;

    fn try_from(value: &DkgRound1) -> Result<Self, Error> {
        Ok(Self {
            dealer: value.dealer,
            commitments: Commitments::new(parse_points(&value.commitments)?)?,
            proof: ProofOfKnowledge {
                commitment: Point::try_from(&value.proof_commitment)?,
                response: parse_scalar(&value.proof_response)?,
            },
        })
    }
}

/// A share sealed from dealer to recipient: a DKG round 2 share or a reshare sub-share.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedShare {
    /// Dealer index.
    pub dealer: MemberIndex,
    /// Recipient index.
    pub recipient: MemberIndex,
    /// HPKE encapsulated key.
    pub encapsulated_key: [u8; 32],
    /// AEAD ciphertext.
    pub ciphertext: Vec<u8>,
}

impl SealedShare {
    /// Wraps an envelope.
    #[must_use]
    pub fn new(dealer: MemberIndex, recipient: MemberIndex, envelope: Envelope) -> Self {
        Self {
            dealer,
            recipient,
            encapsulated_key: envelope.encapsulated_key,
            ciphertext: envelope.ciphertext,
        }
    }

    /// The envelope, to open with `ome_crypto::sealing::envelope::open_scalar`.
    #[must_use]
    pub fn envelope(&self) -> Envelope {
        Envelope {
            encapsulated_key: self.encapsulated_key,
            ciphertext: self.ciphertext.clone(),
        }
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded(
            "ciphertext",
            self.ciphertext.len(),
            1,
            MAX_SEALED_SHARE_BYTES,
        )
    }
}

/// Transcript confirmation: every member must report the same hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirm {
    /// Transcript hash.
    pub transcript: Bytes64,
}

/// Reshare dealer commitments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReshareDeal {
    /// Dealer index in the current committee.
    pub dealer: MemberIndex,
    /// Feldman commitments; the constant term must be `λ_dealer · Y_dealer`.
    pub commitments: Vec<WirePoint>,
}

impl ReshareDeal {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("commitments", self.commitments.len(), 1, MAX_MEMBERS)
    }
}

impl From<&DealerPackage> for ReshareDeal {
    fn from(package: &DealerPackage) -> Self {
        Self {
            dealer: package.dealer,
            commitments: package
                .commitments
                .points()
                .iter()
                .map(WirePoint::from)
                .collect(),
        }
    }
}

impl TryFrom<&ReshareDeal> for DealerPackage {
    type Error = Error;

    fn try_from(value: &ReshareDeal) -> Result<Self, Error> {
        Ok(Self {
            dealer: value.dealer,
            commitments: Commitments::new(parse_points(&value.commitments)?)?,
        })
    }
}

/// A member accuses `accused` of dealing it a share that does not match the accused's commitments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Complaint {
    /// Ceremony.
    pub ceremony: Ceremony,
    /// Accused dealer.
    pub accused: MemberIndex,
}

/// The accused's answer: the share it dealt to `complainant`, revealed so everyone can check it against the accused's
/// commitments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Justification {
    /// Ceremony.
    pub ceremony: Ceremony,
    /// The complaining member.
    pub complainant: MemberIndex,
    /// The share the accused dealt to the complainant.
    pub share: WireScalar,
}

impl Justification {
    /// `true` if the revealed share matches the accused's commitments (the complaint was false), `false` if the accused
    /// dealt a bad share.
    ///
    /// # Errors
    /// [`Error::Crypto`] for a non-canonical share.
    pub fn holds(&self, accused_commitments: &Commitments) -> Result<bool, Error> {
        let share = SecretScalar::new(parse_scalar(&self.share)?);
        Ok(accused_commitments.verify_share(self.complainant, &share))
    }
}

/// Why a ceremony was abandoned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbortReason {
    /// A round timed out.
    Timeout,
    /// A complaint was upheld.
    Complaint,
    /// A member sent conflicting messages.
    Equivocation,
    /// A member cancelled.
    Cancelled,
}

/// A ceremony is abandoned; members discard its state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Abort {
    /// Ceremony.
    pub ceremony: Ceremony,
    /// Reason.
    pub reason: AbortReason,
}

/// A dealer's signed ceremony broadcast (DKG round 1 or reshare deal), forwarded verbatim by another participant, so
/// every participant sees every version of it. Two different broadcasts signed by the same dealer prove equivocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Echo {
    /// The dealer's encoded `SignedMessage`.
    pub frame: Vec<u8>,
}

impl Echo {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("frame", self.frame.len(), 1, MAX_ECHO_BYTES)
    }
}
