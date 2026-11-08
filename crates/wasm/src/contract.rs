//! The contract mirror: what `OmeVault` computes, byte for byte, so a member checks a witness or commitment before
//! sending it.

use ome_api::{Hex32, Hex64};
use ome_crypto::schnorr;
use ome_protocol::vault::{self, Committee, SignerSet, Witness};
use wasm_bindgen::prelude::*;

use crate::{
    convert,
    error::Result,
    js::{export, from_js, seconds},
};

/// What each signer signs to approve: `Poseidon2("ome.approval.v0", [vault, outerHash, configVersion, expiresAt])`.
#[wasm_bindgen(js_name = approvalMessage, unchecked_return_type = "Hex32")]
pub fn approval_message(
    #[wasm_bindgen(unchecked_param_type = "Hex32")] vault: &str,
    #[wasm_bindgen(js_name = outerHash, unchecked_param_type = "Hex32")] outer_hash: &str,
    #[wasm_bindgen(js_name = configVersion)] config_version: u32,
    #[wasm_bindgen(js_name = expiresAt)] expires_at: f64,
) -> Result<JsValue, JsValue> {
    export(|| {
        Ok(Hex32::from_field(&vault::approval_message(
            convert::field(vault)?,
            convert::field(outer_hash)?,
            config_version,
            seconds(expires_at)?,
        )))
    })
}

/// What a member signs to reject proposal `proposalId`.
#[wasm_bindgen(js_name = rejectionMessage, unchecked_return_type = "Hex32")]
pub fn rejection_message(
    #[wasm_bindgen(unchecked_param_type = "Hex32")] vault: &str,
    #[wasm_bindgen(js_name = proposalId, unchecked_param_type = "Hex32")] proposal_id: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        Ok(Hex32::from_field(&vault::rejection_message(
            convert::field(vault)?,
            &convert::hex32(proposal_id)?.0,
        )))
    })
}

/// The commitment to the ordered signer keys stored in the config note.
#[wasm_bindgen(js_name = signerSetCommitment, unchecked_return_type = "Hex32")]
pub fn signer_set_commitment(
    #[wasm_bindgen(unchecked_param_type = "Point[]")] keys: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let signers = SignerSet::new(convert::points(from_js(keys)?)?)?;
        Ok(Hex32::from_field(&signers.commitment()))
    })
}

/// The commitment to the viewing committee: `t_v` and the verification shares in member order.
#[wasm_bindgen(js_name = committeeCommitment, unchecked_return_type = "Hex32")]
pub fn committee_commitment(
    #[wasm_bindgen(js_name = viewingThreshold)] viewing_threshold: u32,
    #[wasm_bindgen(unchecked_param_type = "Point[]")] shares: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let committee = Committee::new(viewing_threshold, convert::points(from_js(shares)?)?)?;
        Ok(Hex32::from_field(&committee.commitment()))
    })
}

/// The 49-field auth witness: signer keys, one signature per slot (`null` where the signer did not approve) and the
/// deadline.
#[wasm_bindgen(js_name = encodeWitness, unchecked_return_type = "Hex32[]")]
pub fn encode_witness(
    #[wasm_bindgen(js_name = signerKeys, unchecked_param_type = "Point[]")] signer_keys: JsValue,
    #[wasm_bindgen(unchecked_param_type = "(Hex64 | null)[]")] signatures: JsValue,
    #[wasm_bindgen(js_name = expiresAt)] expires_at: f64,
) -> Result<JsValue, JsValue> {
    export(|| {
        let signers = SignerSet::new(convert::points(from_js(signer_keys)?)?)?;
        let signatures = from_js::<Vec<Option<Hex64>>>(signatures)?
            .into_iter()
            .map(|s| s.map(|s| schnorr::Signature::from_bytes(&s.0)).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(witness_fields(&Witness::encode(
            &signers,
            &signatures,
            seconds(expires_at)?,
        )))
    })
}

pub(crate) fn witness_fields(witness: &Witness) -> Vec<Hex32> {
    witness.0.iter().map(Hex32::from_field).collect()
}
