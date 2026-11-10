//! Proposals and votes: building a proposal, checking one before review, decoding and verifying votes, and tallying
//! them into the auth witness. Bodies are hex of their canonical `ome-wire` encoding, as the keeper stores them.

use ome_api::{Hex32, HexBytes};
use ome_protocol::{
    payload::ExecutionRequest,
    proposals::{self, Draft, Status, Tally},
};
use ome_wire::Message;
use wasm_bindgen::prelude::*;

use crate::{
    contract::witness_fields,
    convert,
    error::Result,
    js::{export, from_js, seconds},
    types::{
        ApprovalInfo, BuiltProposal, ProposalInfo, RefusedVote, RejectionInfo, TallyResult,
        TallyStatus,
    },
};

/// Builds a proposal and its payload from a draft, computing the outer hash from the calls.
#[wasm_bindgen(js_name = buildProposal, unchecked_return_type = "BuiltProposal")]
pub fn build_proposal(
    #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Chain")] chain: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Draft")] draft: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let vault = convert::vault(from_js(vault)?)?;
        let draft: crate::types::Draft = from_js(draft)?;
        let draft = Draft {
            request: ExecutionRequest {
                calls: draft.calls.into_iter().map(convert::call).collect(),
                extras: draft.extras.0,
            },
            tx_nonce: draft.tx_nonce.to_field()?,
            created_at: draft.created_at,
            expires_at: draft.expires_at,
            summary: draft.summary,
            proposer: draft.proposer,
        };
        let (proposal, payload) = draft.build(&vault, convert::chain(from_js(chain)?)?)?;
        Ok(BuiltProposal {
            id: proposal.id()?.into(),
            outer_hash: proposal.outer_hash.into(),
            approval_message: Hex32::from_field(&proposals::approval_message_for(
                &vault, &proposal,
            )?),
            proposal: convert::body(&Message::Proposal(proposal))?,
            payload: convert::body(&Message::Payload(payload))?,
        })
    })
}

/// Checks a received proposal before review: the payload is the one referenced, hashes to the outer hash on this vault
/// and chain, the config is current, the deadline has not passed and fees stay off the vault. Returns the calls.
#[wasm_bindgen(js_name = checkProposal, unchecked_return_type = "ExecutionRequest")]
pub fn check_proposal(
    #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Chain")] chain: JsValue,
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] proposal: &str,
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] payload: &str,
    now: f64,
) -> Result<JsValue, JsValue> {
    export(|| {
        let request = proposals::check_proposal(
            &convert::vault(from_js(vault)?)?,
            convert::chain(from_js(chain)?)?,
            &convert::proposal(proposal)?,
            &convert::payload(payload)?,
            seconds(now)?,
        )?;
        Ok(crate::types::ExecutionRequest {
            calls: request.calls.iter().map(convert::call_out).collect(),
            extras: HexBytes(request.extras),
        })
    })
}

/// A proposal's fields.
#[wasm_bindgen(js_name = decodeProposal, unchecked_return_type = "ProposalInfo")]
pub fn decode_proposal(
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] proposal: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        let p = convert::proposal(proposal)?;
        Ok(ProposalInfo {
            id: p.id()?.into(),
            config_version: p.config_version,
            outer_hash: p.outer_hash.into(),
            created_at: p.created_at,
            expires_at: p.expires_at,
            tx_nonce: p.tx_nonce.into(),
            fee_method: p.fee_method,
            payload_hash: p.payload.hash.into(),
            payload_len: p.payload.len,
            summary: p.summary,
            proposer: p.proposer,
        })
    })
}

/// An approval's fields. The signature is not checked; see [`verify_approval`].
#[wasm_bindgen(js_name = decodeApproval, unchecked_return_type = "ApprovalInfo")]
pub fn decode_approval(
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] approval: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        let a = convert::approval(approval)?;
        Ok(ApprovalInfo {
            proposal: a.proposal.into(),
            slot: a.slot,
            signature: convert::signature(&a.signature)?,
        })
    })
}

/// A rejection's fields. The signature is not checked; see [`verify_rejection`].
#[wasm_bindgen(js_name = decodeRejection, unchecked_return_type = "RejectionInfo")]
pub fn decode_rejection(
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] rejection: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        let r = convert::rejection(rejection)?;
        Ok(RejectionInfo {
            proposal: r.proposal.into(),
            slot: r.slot,
            signature: convert::signature(&r.signature)?,
            reason: r.reason,
        })
    })
}

/// Throws unless `approval` is for `proposal` and signed by the key in its slot.
#[wasm_bindgen(js_name = verifyApproval)]
pub fn verify_approval(
    #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] proposal: &str,
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] approval: &str,
) -> Result<(), JsValue> {
    let run = || -> Result<()> {
        Ok(proposals::verify_approval(
            &convert::vault(from_js(vault)?)?,
            &convert::proposal(proposal)?,
            &convert::approval(approval)?,
        )?)
    };
    run().map_err(JsValue::from)
}

/// Throws unless `rejection` is for `proposal` and signed by the key in its slot.
#[wasm_bindgen(js_name = verifyRejection)]
pub fn verify_rejection(
    #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] proposal: &str,
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] rejection: &str,
) -> Result<(), JsValue> {
    let run = || -> Result<()> {
        Ok(proposals::verify_rejection(
            &convert::vault(from_js(vault)?)?,
            &convert::proposal(proposal)?.id()?,
            &convert::rejection(rejection)?,
        )?)
    };
    run().map_err(JsValue::from)
}

/// Counts the votes on a proposal at `now`: each slot's first valid vote counts, invalid ones are listed, and the
/// witness is built once `k` members approved.
#[wasm_bindgen(unchecked_return_type = "TallyResult")]
pub fn tally(
    #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] proposal: &str,
    #[wasm_bindgen(unchecked_param_type = "HexBytes[]")] approvals: JsValue,
    #[wasm_bindgen(unchecked_param_type = "HexBytes[]")] rejections: JsValue,
    now: f64,
) -> Result<JsValue, JsValue> {
    export(|| {
        let vault = convert::vault(from_js(vault)?)?;
        let proposal = convert::proposal(proposal)?;
        let mut tally = Tally::new(&vault, &proposal)?;
        let mut refused = Vec::new();
        let (mut approved, mut rejected) = (0, 0);
        for (index, body) in (0u32..).zip(from_js::<Vec<String>>(approvals)?) {
            match convert::approval(&body).and_then(|a| Ok(tally.add_approval(&a)?)) {
                Ok(counted) => approved += u32::from(counted),
                Err(error) => refused.push(RefusedVote {
                    index,
                    approval: true,
                    reason: error.to_string(),
                }),
            }
        }
        for (index, body) in (0u32..).zip(from_js::<Vec<String>>(rejections)?) {
            match convert::rejection(&body).and_then(|r| Ok(tally.add_rejection(&r)?)) {
                Ok(counted) => rejected += u32::from(counted),
                Err(error) => refused.push(RefusedVote {
                    index,
                    approval: false,
                    reason: error.to_string(),
                }),
            }
        }
        let status = match tally.status(seconds(now)?) {
            Status::Pending { .. } => TallyStatus::Pending,
            Status::Approved => TallyStatus::Approved,
            Status::Blocked => TallyStatus::Blocked,
            Status::Expired => TallyStatus::Expired,
        };
        Ok(TallyResult {
            status,
            approvals: approved,
            rejections: rejected,
            needed: vault.config().threshold,
            refused,
            witness: tally.witness().as_ref().map(witness_fields),
        })
    })
}
