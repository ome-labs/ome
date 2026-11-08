//! Conversions between the boundary shapes and the core types.

use ome_api::{Hex32, Hex64, HexBytes, Point};
use ome_crypto::schnorr;
use ome_protocol::{membership, payload, vault};
use ome_wire::{
    Message, decode_body,
    messages::{ceremony, proposals},
};

use crate::{
    error::{Result, WasmError},
    types::{Call, CallKind, Chain, VaultInput},
};

pub(crate) fn points(points: Vec<Point>) -> Result<Vec<ome_crypto::Point>> {
    Ok(points
        .into_iter()
        .map(Point::to_core)
        .collect::<Result<_, _>>()?)
}

pub(crate) fn vault(input: VaultInput) -> Result<membership::Vault> {
    let manifest = ceremony::Manifest::try_from(input.manifest)?;
    let config = vault::Config::try_from(input.config)?;
    Ok(membership::Vault::from_manifest(&manifest, &config)?)
}

pub(crate) fn chain(chain: Chain) -> Result<payload::ChainInfo> {
    Ok(payload::ChainInfo {
        chain_id: chain.chain_id.to_field()?,
        version: chain.version.to_field()?,
    })
}

pub(crate) fn call(call: Call) -> payload::Call {
    payload::Call {
        signature: call.signature,
        to: call.to.0,
        selector: call.selector,
        kind: match call.kind {
            CallKind::Private => payload::CallKind::Private,
            CallKind::Public => payload::CallKind::Public,
        },
        is_static: call.is_static,
        hide_msg_sender: call.hide_msg_sender,
        args: call.args.into_iter().map(|arg| arg.0).collect(),
    }
}

pub(crate) fn call_out(call: &payload::Call) -> Call {
    Call {
        signature: call.signature.clone(),
        to: call.to.into(),
        selector: call.selector,
        kind: match call.kind {
            payload::CallKind::Private => CallKind::Private,
            payload::CallKind::Public => CallKind::Public,
        },
        is_static: call.is_static,
        hide_msg_sender: call.hide_msg_sender,
        args: call.args.iter().copied().map(Hex32).collect(),
    }
}

pub(crate) fn signature(signature: &ome_wire::types::WireSignature) -> Result<Hex64> {
    Ok(Hex64(schnorr::Signature::try_from(signature)?.to_bytes()))
}

/// Reads a hex param into bytes.
pub(crate) fn bytes(hex: &str) -> Result<Vec<u8>> {
    hex.parse::<HexBytes>()
        .map(|bytes| bytes.0)
        .map_err(|e| WasmError::invalid(e.to_string()))
}

/// Reads a hex param into a 32-byte value.
pub(crate) fn hex32(hex: &str) -> Result<Hex32> {
    hex.parse()
        .map_err(|e: ome_api::HexError| WasmError::invalid(e.to_string()))
}

/// Reads a hex param into a field element.
pub(crate) fn field(hex: &str) -> Result<ome_crypto::Base> {
    Ok(hex32(hex)?.to_field()?)
}

pub(crate) fn proposal(hex: &str) -> Result<proposals::Proposal> {
    match decode_body(&bytes(hex)?)? {
        Message::Proposal(proposal) => Ok(proposal),
        _ => Err(WasmError::invalid("not a proposal")),
    }
}

pub(crate) fn payload(hex: &str) -> Result<proposals::ProposalPayload> {
    match decode_body(&bytes(hex)?)? {
        Message::Payload(payload) => Ok(payload),
        _ => Err(WasmError::invalid("not a payload")),
    }
}

pub(crate) fn approval(hex: &str) -> Result<proposals::Approval> {
    match decode_body(&bytes(hex)?)? {
        Message::Approval(approval) => Ok(approval),
        _ => Err(WasmError::invalid("not an approval")),
    }
}

pub(crate) fn rejection(hex: &str) -> Result<proposals::Rejection> {
    match decode_body(&bytes(hex)?)? {
        Message::Rejection(rejection) => Ok(rejection),
        _ => Err(WasmError::invalid("not a rejection")),
    }
}

pub(crate) fn body(message: &Message) -> Result<HexBytes> {
    Ok(HexBytes(ome_wire::encode_body(message)?))
}
