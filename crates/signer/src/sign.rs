//! The only ways to use a signing key. Each runs `ome-protocol`'s checks before signing, so a compromised proposer or
//! front end cannot get a member's signature on a transaction other than the one the payload describes.

use ome_protocol::{
    membership::{Rotation, Vault, check_rotation, check_signer_swap, is_rotation_call},
    payload::{Call, ChainInfo},
    proposals::{approval_messages_for, check_proposal},
    vault::rejection_message,
};
use ome_wire::{
    limits::MAX_REASON_BYTES,
    messages::proposals::{Approval, Proposal, ProposalPayload, Rejection},
    types::{WireField, WireSignature},
};
use rand_core::CryptoRng;

use crate::{Error, Message, Signer};

/// Approves `proposal` after [`check_proposal`]: the payload is the one referenced and hashes to the proposal's outer
/// hash and authwit hashes on this vault and chain, the config is current, the deadline has not passed (`now` is unix
/// seconds) and fees stay off the vault. The approval signs the transaction and every authwit it grants. A proposal
/// that rotates the vault's membership is refused here; use [`approve_rotation`].
///
/// # Errors
/// [`Error::NotASigner`], [`Error::RotationNeedsReview`], or the protocol error that refused the proposal.
pub fn approve<S: Signer>(
    signer: &S,
    vault: &Vault,
    chain: ChainInfo,
    proposal: &Proposal,
    payload: &ProposalPayload,
    now: u64,
    rng: &mut impl CryptoRng,
) -> Result<Approval, Error> {
    let slot = slot_of(signer, vault)?;
    let request = check_proposal(vault, chain, proposal, payload, now)?;
    if request
        .calls
        .iter()
        .any(|call| is_rotation_call(call, vault))
    {
        return Err(Error::RotationNeedsReview);
    }
    sign_approval(signer, vault, proposal, slot, rng)
}

/// Approves a membership rotation the member reviewed: the proposal passes [`check_proposal`], its payload is exactly
/// one `rotate_membership` call performing `rotation`, and `rotation` passes [`check_rotation`] (the contract's rules,
/// shares that hold the vault's key, and fresh shares).
///
/// The payload may also carry the fee payment: calls to one of `fee_payers`, the fee payer contracts the member's
/// own configuration trusts (never the proposer's). A sponsored fee payer is paid through such a call, so without it no
/// rotation could pay its fee without the vault paying publicly. Any other call is refused, so a rotation cannot carry
/// a transfer.
///
/// # Errors
/// [`Error::NotASigner`], [`Error::NotARotation`], [`Error::UnexpectedCall`], [`Error::RotationGrantsAuthwit`],
/// [`Error::RotationMismatch`], or the protocol error that refused the proposal or rotation.
#[allow(clippy::too_many_arguments)]
pub fn approve_rotation<S: Signer>(
    signer: &S,
    vault: &Vault,
    chain: ChainInfo,
    proposal: &Proposal,
    payload: &ProposalPayload,
    rotation: &Rotation,
    fee_payers: &[WireField],
    now: u64,
    rng: &mut impl CryptoRng,
) -> Result<Approval, Error> {
    let slot = slot_of(signer, vault)?;
    let call = single_rotation(vault, chain, proposal, payload, fee_payers, now)?;
    if &Rotation::from_call(&call, vault)? != rotation {
        return Err(Error::RotationMismatch);
    }
    check_rotation(vault, rotation)?;
    sign_approval(signer, vault, proposal, slot, rng)
}

/// Approves a signer swap: the proposal passes [`check_proposal`], and its payload is exactly one `rotate_membership`
/// call that replaces one member's key and changes nothing else ([`check_signer_swap`]), plus at most calls to one of
/// `fee_payers`, as for [`approve_rotation`]. Returns the approval and the swapped slot.
///
/// The committee stays, so there is no reshare to compare with: a swap needs only the contract's rules and its shape.
///
/// # Errors
/// [`Error::NotASigner`], [`Error::NotARotation`], [`Error::UnexpectedCall`], [`Error::RotationGrantsAuthwit`], or the
/// protocol error that refused the proposal or swap.
#[allow(clippy::too_many_arguments)]
pub fn approve_signer_swap<S: Signer>(
    signer: &S,
    vault: &Vault,
    chain: ChainInfo,
    proposal: &Proposal,
    payload: &ProposalPayload,
    fee_payers: &[WireField],
    now: u64,
    rng: &mut impl CryptoRng,
) -> Result<(Approval, u32), Error> {
    let slot = slot_of(signer, vault)?;
    let call = single_rotation(vault, chain, proposal, payload, fee_payers, now)?;
    let (swapped, _) = check_signer_swap(vault, &Rotation::from_call(&call, vault)?)?;
    Ok((sign_approval(signer, vault, proposal, slot, rng)?, swapped))
}

/// The proposal's one `rotate_membership` call, after [`check_proposal`]; any other call must go to a trusted fee payer,
/// and the proposal grants no authwit.
fn single_rotation(
    vault: &Vault,
    chain: ChainInfo,
    proposal: &Proposal,
    payload: &ProposalPayload,
    fee_payers: &[WireField],
    now: u64,
) -> Result<Call, Error> {
    let request = check_proposal(vault, chain, proposal, payload, now)?;
    if !request.authwits.is_empty() {
        return Err(Error::RotationGrantsAuthwit);
    }
    let (rotations, others): (Vec<_>, Vec<_>) = request
        .calls
        .into_iter()
        .partition(|call| is_rotation_call(call, vault));
    let [call] = <[Call; 1]>::try_from(rotations).map_err(|_| Error::NotARotation)?;
    if let Some(other) = others.iter().find(|call| !fee_payers.contains(&call.to)) {
        return Err(Error::UnexpectedCall(hex::encode(other.to)));
    }
    Ok(call)
}

/// Rejects `proposal` with a reason shown to the other members.
///
/// # Errors
/// [`Error::NotASigner`] or [`Error::ReasonTooLong`].
pub fn reject<S: Signer>(
    signer: &S,
    vault: &Vault,
    proposal: &Proposal,
    reason: &str,
    rng: &mut impl CryptoRng,
) -> Result<Rejection, Error> {
    let slot = slot_of(signer, vault)?;
    if reason.len() > MAX_REASON_BYTES {
        return Err(Error::ReasonTooLong(MAX_REASON_BYTES));
    }
    let id = proposal.id()?;
    let signature = signer.sign(&Message::new(rejection_message(vault.address(), &id)), rng)?;
    Ok(Rejection {
        proposal: id,
        slot,
        reason: reason.to_owned(),
        signature: WireSignature::from(&signature),
        extensions: ome_wire::Extensions::none(),
    })
}

fn sign_approval<S: Signer>(
    signer: &S,
    vault: &Vault,
    proposal: &Proposal,
    slot: u32,
    rng: &mut impl CryptoRng,
) -> Result<Approval, Error> {
    let signatures = approval_messages_for(vault, proposal)?
        .into_iter()
        .map(|message| signer.sign(&Message::new(message), rng))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Approval::new(proposal.id()?, slot, &signatures))
}

/// The signer's slot in the vault's signer set.
fn slot_of<S: Signer>(signer: &S, vault: &Vault) -> Result<u32, Error> {
    let key = signer.public_key();
    let position = vault
        .signers()
        .keys()
        .iter()
        .position(|k| *k == key)
        .ok_or(Error::NotASigner)?;
    u32::try_from(position).map_err(|_| Error::NotASigner)
}
