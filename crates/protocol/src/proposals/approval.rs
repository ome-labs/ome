use ome_crypto::{Base, schnorr};
use ome_wire::{
    messages::proposals::{Approval, Proposal, Rejection},
    types::parse_field,
};

use crate::{
    Error,
    membership::Vault,
    vault::{approval_message, rejection_message},
};

/// The messages a member signs to approve `proposal` on `vault`: the transaction's outer hash first, then each
/// authwit's message hash, each bound to the vault, config version and deadline as the contract checks it.
///
/// # Errors
/// [`ome_wire::Error`] for a non-canonical hash.
pub fn approval_messages_for(vault: &Vault, proposal: &Proposal) -> Result<Vec<Base>, Error> {
    std::iter::once(&proposal.outer_hash)
        .chain(&proposal.authwits)
        .map(|hash| {
            Ok(approval_message(
                vault.address(),
                parse_field(hash)?,
                proposal.config_version,
                proposal.expires_at,
            ))
        })
        .collect()
}

/// Checks that `approval` is for `proposal` and that each of its signatures, one per approval message, is by the key
/// in its slot. An approval copied from another proposal, vault, config version, deadline or slot fails.
///
/// # Errors
/// [`Error::WrongProposal`], [`Error::SignatureCount`], [`Error::UnknownSlot`] or [`Error::InvalidSignature`].
pub fn verify_approval(
    vault: &Vault,
    proposal: &Proposal,
    approval: &Approval,
) -> Result<(), Error> {
    if approval.proposal != proposal.id()? {
        return Err(Error::WrongProposal);
    }
    let messages = approval_messages_for(vault, proposal)?;
    if approval.signatures.len() != messages.len() {
        return Err(Error::SignatureCount {
            slot: approval.slot,
            expected: messages.len(),
            got: approval.signatures.len(),
        });
    }
    messages
        .into_iter()
        .zip(&approval.signatures)
        .try_for_each(|(message, signature)| verify_slot(vault, approval.slot, message, signature))
}

/// Checks that `rejection` is for the proposal `proposal_id` and signed by the key in its slot.
///
/// # Errors
/// [`Error::WrongProposal`], [`Error::UnknownSlot`] or [`Error::InvalidSignature`].
pub fn verify_rejection(
    vault: &Vault,
    proposal_id: &[u8; 32],
    rejection: &Rejection,
) -> Result<(), Error> {
    if &rejection.proposal != proposal_id {
        return Err(Error::WrongProposal);
    }
    verify_slot(
        vault,
        rejection.slot,
        rejection_message(vault.address(), proposal_id),
        &rejection.signature,
    )
}

fn verify_slot(
    vault: &Vault,
    slot: u32,
    message: Base,
    signature: &ome_wire::types::WireSignature,
) -> Result<(), Error> {
    let key = vault.signers().key(slot).ok_or(Error::UnknownSlot(slot))?;
    let signature =
        schnorr::Signature::try_from(signature).map_err(|_| Error::InvalidSignature(slot))?;
    if schnorr::verify(key, message, &signature) {
        Ok(())
    } else {
        Err(Error::InvalidSignature(slot))
    }
}
