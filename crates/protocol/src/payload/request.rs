use ome_crypto::Base;
use ome_wire::{
    codec::{from_canonical, to_canonical},
    limits::MAX_PAYLOAD_BYTES,
    messages::proposals::ProposalPayload,
    types::parse_field,
};
use serde::{Deserialize, Serialize};

use super::{Authwit, Call, ChainInfo, app_payload_hash, outer_hash};
use crate::{
    Error,
    vault::constants::{APP_MAX_AUTHWITS, APP_MAX_CALLS},
};

/// What a proposal's payload carries: the calls members sign, the private authwits granted with them, and the PXE's
/// execution data.
///
/// The payload bytes are `canonical(ExecutionRequest)`, so they have exactly one form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRequest {
    /// The calls, at most five. None is a valid cancellation: it burns the nonce.
    pub calls: Vec<Call>,
    /// Private authwits the vault grants with the calls, at most four, each signed beside the transaction. Every
    /// caller is the target of one of the calls, so an authwit can only be used inside this transaction's own flow.
    pub authwits: Vec<Authwit>,
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

    /// The message hashes of the authwits on `chain`, in order.
    ///
    /// # Errors
    /// Field encoding errors.
    pub fn authwit_hashes(&self, chain: ChainInfo) -> Result<Vec<Base>, Error> {
        self.authwits
            .iter()
            .map(|authwit| authwit.message_hash(chain))
            .collect()
    }

    /// Checks the authwits against the vault: each caller is the target of one of the calls other than the vault (whose
    /// own calls need no authwit), and no authwit is consumed by the vault itself.
    ///
    /// # Errors
    /// [`Error::InvalidAuthwit`].
    pub fn check_authwits(&self, vault: Base) -> Result<(), Error> {
        let targets = self
            .calls
            .iter()
            .map(|call| parse_field(&call.to))
            .collect::<Result<Vec<_>, _>>()?;
        for (index, authwit) in self.authwits.iter().enumerate() {
            let invalid = |reason| Error::InvalidAuthwit { index, reason };
            let caller = parse_field(&authwit.caller)?;
            if caller == vault {
                return Err(invalid("the vault needs no authwit for its own calls"));
            }
            if !targets.contains(&caller) {
                return Err(invalid("the caller is not called by the transaction"));
            }
            if parse_field(&authwit.call.to)? == vault {
                return Err(invalid("the vault cannot consume its own authwit"));
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), Error> {
        if self.calls.len() > APP_MAX_CALLS {
            return Err(Error::TooManyCalls(self.calls.len()));
        }
        if self.authwits.len() > APP_MAX_AUTHWITS {
            return Err(Error::TooManyAuthwits(self.authwits.len()));
        }
        self.calls
            .iter()
            .enumerate()
            .try_for_each(|(index, call)| call.validate(index))?;
        self.authwits
            .iter()
            .enumerate()
            .try_for_each(|(index, authwit)| authwit.validate(index))
    }
}
