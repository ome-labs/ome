use ome_crypto::Base;
use ome_wire::{
    messages::proposals::{Proposal, ProposalPayload},
    types::{field, parse_field},
};

use crate::{
    Error,
    membership::Vault,
    payload::{ChainInfo, ExecutionRequest},
};

/// Fee method `EXTERNAL`: another contract (a fee paying contract or sponsor) pays. The vault's other fee methods
/// make it the public fee payer, which would link its transactions on chain, so Ome proposals use only this one.
pub const FEE_EXTERNAL: u8 = 0;

/// What a member proposes: the calls and the proposal's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    /// The calls and execution data.
    pub request: ExecutionRequest,
    /// Entrypoint nonce; cancelling the proposal burns it.
    pub tx_nonce: Base,
    /// When it is made (unix seconds, the clock `expires_at` uses).
    pub created_at: u64,
    /// Approval deadline (unix seconds).
    pub expires_at: u64,
    /// Human-readable summary.
    pub summary: String,
    /// Proposing member (1-based).
    pub proposer: u32,
}

impl Draft {
    /// The proposal and payload to send, with the outer hash computed from the calls.
    ///
    /// # Errors
    /// Call and payload errors.
    pub fn build(
        &self,
        vault: &Vault,
        chain: ChainInfo,
    ) -> Result<(Proposal, ProposalPayload), Error> {
        let payload = self.request.to_payload()?;
        let proposal = Proposal {
            config_version: vault.config().version,
            outer_hash: field(
                &self
                    .request
                    .outer_hash(vault.address(), chain, self.tx_nonce)?,
            ),
            created_at: self.created_at,
            expires_at: self.expires_at,
            tx_nonce: field(&self.tx_nonce),
            fee_method: FEE_EXTERNAL,
            payload: payload.reference()?,
            summary: self.summary.clone(),
            proposer: self.proposer,
        };
        Ok((proposal, payload))
    }
}

/// Checks a received proposal before a member reviews or signs it, and returns the calls to show them:
///
/// - the payload is the one referenced, and decodes to a valid execution request;
/// - the calls hash to the proposal's outer hash on this vault and chain;
/// - it is for the current config version, its deadline has not passed (`now` is unix seconds), the proposer is a
///   member, and the fee method keeps fees off the vault.
///
/// # Errors
/// [`Error::PayloadMismatch`], [`Error::OuterHashMismatch`], [`Error::StaleConfig`], [`Error::Expired`],
/// [`Error::UnknownSlot`], [`Error::PublicFeePayment`], or decoding errors.
pub fn check_proposal(
    vault: &Vault,
    chain: ChainInfo,
    proposal: &Proposal,
    payload: &ProposalPayload,
    now: u64,
) -> Result<ExecutionRequest, Error> {
    if !proposal.payload.matches(payload) {
        return Err(Error::PayloadMismatch);
    }
    let request = ExecutionRequest::from_payload(payload)?;
    let outer_hash =
        request.outer_hash(vault.address(), chain, parse_field(&proposal.tx_nonce)?)?;
    if outer_hash != parse_field(&proposal.outer_hash)? {
        return Err(Error::OuterHashMismatch);
    }
    let current = vault.config().version;
    if proposal.config_version != current {
        return Err(Error::StaleConfig {
            proposed: proposal.config_version,
            current,
        });
    }
    if now > proposal.expires_at {
        return Err(Error::Expired {
            at: proposal.expires_at,
        });
    }
    if proposal.proposer == 0 || proposal.proposer > vault.signers().count() {
        return Err(Error::UnknownSlot(proposal.proposer.saturating_sub(1)));
    }
    if proposal.fee_method != FEE_EXTERNAL {
        return Err(Error::PublicFeePayment(proposal.fee_method));
    }
    Ok(request)
}
