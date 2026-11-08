use ome_crypto::Base;
use ome_wire::{
    codec::{from_canonical, to_canonical},
    limits::MAX_PAYLOAD_BYTES,
    messages::proposals::ProposalPayload,
};
use serde::{Deserialize, Serialize};

use super::{Call, ChainInfo, app_payload_hash, outer_hash};
use crate::{Error, vault::constants::APP_MAX_CALLS};

/// What a proposal's payload carries: the calls members sign, and the PXE's execution data.
///
/// The payload bytes are `canonical(ExecutionRequest)`, so they have exactly one form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRequest {
    /// The calls, at most five. None is a valid cancellation: it burns the nonce.
    pub calls: Vec<Call>,
    /// Execution data the PXE needs but the signature does not cover (auth witnesses, capsules, extra hashed
    /// arguments), encoded by the daemon. It cannot change which calls run or with which arguments; contracts must
    /// constrain any hint they read from it.
    pub extras: Vec<u8>,
}

impl ExecutionRequest {
    /// Encodes the request as a proposal payload.
    ///
    /// # Errors
    /// Call errors, or [`ome_wire::Error::TooLarge`] above the payload limit.
    pub fn to_payload(&self) -> Result<ProposalPayload, Error> {
        self.validate()?;
        Ok(ProposalPayload {
            bytes: to_canonical(self, MAX_PAYLOAD_BYTES)?,
        })
    }

    /// Decodes and checks a proposal payload.
    ///
    /// # Errors
    /// [`ome_wire::Error::Malformed`] for bytes that are not a canonical request, call errors.
    pub fn from_payload(payload: &ProposalPayload) -> Result<Self, Error> {
        let request: Self = from_canonical(&payload.bytes, MAX_PAYLOAD_BYTES)?;
        request.validate()?;
        Ok(request)
    }

    /// The hash the vault authorizes for these calls with `tx_nonce`, on `vault` and `chain`.
    ///
    /// # Errors
    /// Call encoding errors.
    pub fn outer_hash(&self, vault: Base, chain: ChainInfo, tx_nonce: Base) -> Result<Base, Error> {
        Ok(outer_hash(
            vault,
            chain,
            app_payload_hash(&self.calls, tx_nonce)?,
        ))
    }

    fn validate(&self) -> Result<(), Error> {
        if self.calls.len() > APP_MAX_CALLS {
            return Err(Error::TooManyCalls(self.calls.len()));
        }
        self.calls
            .iter()
            .enumerate()
            .try_for_each(|(index, call)| call.validate(index))
    }
}
