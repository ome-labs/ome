//! Threshold ECDH requests and DLEQ-proven partials.

use ome_crypto::{
    Point,
    threshold::{
        ecdh::{DleqProof, Partial},
        shamir::MemberIndex,
    },
};
use serde::{Deserialize, Serialize};

use super::bounded;
use crate::{
    Error,
    limits::MAX_ECDH_POINTS,
    types::{WirePoint, WireScalar, parse_scalar, scalar},
};

/// What the requested `address_secret · P` is for: audit logs and rate limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EcdhPurpose {
    /// Note or message decryption (ephemeral keys).
    NoteDecryption,
    /// Handshake shared secret.
    Handshake,
    /// Recipient-side tagging secret (sender address points).
    RecipientTag,
    /// Sender-side tagging secret (recipient address points).
    SenderTag,
}

/// A batch of points to multiply by the vault's address secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcdhRequest {
    /// Purpose.
    pub purpose: EcdhPurpose,
    /// Input points.
    pub points: Vec<WirePoint>,
}

impl EcdhRequest {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("points", self.points.len(), 1, MAX_ECDH_POINTS)
    }
}

/// One partial with its DLEQ proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePartial {
    /// `D_i`.
    pub point: WirePoint,
    /// DLEQ challenge.
    pub challenge: WireScalar,
    /// DLEQ response.
    pub response: WireScalar,
}

/// A member's partials for a request, in request order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcdhResponse {
    /// The request's header id.
    pub request: [u8; 16],
    /// Responding member.
    pub member: MemberIndex,
    /// Partials, one per requested point.
    pub partials: Vec<WirePartial>,
}

impl EcdhResponse {
    /// Wraps a member's partials.
    #[must_use]
    pub fn new(request: [u8; 16], member: MemberIndex, partials: &[Partial]) -> Self {
        let partials = partials
            .iter()
            .map(|p| WirePartial {
                point: WirePoint::from(&p.point),
                challenge: scalar(&p.proof.challenge),
                response: scalar(&p.proof.response),
            })
            .collect();
        Self {
            request,
            member,
            partials,
        }
    }

    /// The partials, decoded as points and scalars. Their proofs are checked with
    /// `ome_crypto::threshold::ecdh::verify_partial`.
    ///
    /// # Errors
    /// [`Error::Crypto`] for invalid encodings.
    pub fn partials(&self) -> Result<Vec<Partial>, Error> {
        self.partials
            .iter()
            .map(|p| {
                Ok(Partial {
                    index: self.member,
                    point: Point::try_from(&p.point)?,
                    proof: DleqProof {
                        challenge: parse_scalar(&p.challenge)?,
                        response: parse_scalar(&p.response)?,
                    },
                })
            })
            .collect()
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("partials", self.partials.len(), 1, MAX_ECDH_POINTS)
    }
}
