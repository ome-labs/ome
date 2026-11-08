//! Membership checks: a rotation's committee holds the vault's key, the contract would accept the change, and the
//! thresholds' risks.

use ome_protocol::{
    membership::{self, policy},
    vault::{Committee, SignerSet},
};
use wasm_bindgen::prelude::*;

use ome_api::Hex64;

use crate::{
    convert,
    error::{Result, WasmError},
    js::{export, from_js},
    types::{Rotation, Warning},
};

pub(crate) fn rotation(rotation: Rotation) -> Result<membership::Rotation> {
    Ok(membership::Rotation {
        signers: SignerSet::new(convert::points(rotation.signers)?)?,
        threshold: rotation.threshold,
        committee: Committee::new(
            rotation.viewing_threshold,
            convert::points(rotation.verification_shares)?,
        )?,
    })
}

/// Throws unless the verification shares interpolate to `groupKey`: the reshared committee still holds the vault's
/// viewing key.
#[wasm_bindgen(js_name = verifyRotationShares)]
pub fn verify_rotation_shares(
    #[wasm_bindgen(js_name = groupKey, unchecked_param_type = "Point")] group_key: JsValue,
    #[wasm_bindgen(js_name = viewingThreshold)] viewing_threshold: u32,
    #[wasm_bindgen(js_name = verificationShares, unchecked_param_type = "Point[]")]
    verification_shares: JsValue,
) -> Result<(), JsValue> {
    let run = || -> Result<()> {
        let group_key: ome_api::Point = from_js(group_key)?;
        let committee = Committee::new(
            viewing_threshold,
            convert::points(from_js(verification_shares)?)?,
        )?;
        Ok(membership::verify_shares(
            &group_key.to_core()?,
            &committee,
        )?)
    };
    run().map_err(JsValue::from)
}

/// Checks a rotation against the vault as the contract would, and returns the config it would store.
#[wasm_bindgen(js_name = checkRotation, unchecked_return_type = "VaultConfig")]
pub fn check_rotation(
    #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Rotation")] rotation: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let vault = convert::vault(from_js(vault)?)?;
        let config = membership::check_rotation(&vault, &self::rotation(from_js(rotation)?)?)?;
        Ok(ome_api::keeper::VaultConfig::from(&config))
    })
}

/// The `rotate_membership` call that performs a rotation, to propose.
#[wasm_bindgen(js_name = rotationCall, unchecked_return_type = "Call")]
pub fn rotation_call(
    #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Rotation")] rotation: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let vault = convert::vault(from_js(vault)?)?;
        Ok(convert::call_out(
            &self::rotation(from_js(rotation)?)?.to_call(&vault),
        ))
    })
}

/// The signer slot a payload's `rotate_membership` call swaps, if it is a signer swap of the vault (one key replaced,
/// the committee and thresholds unchanged); `null` for a payload that rotates nothing or reshares.
#[wasm_bindgen(js_name = signerSwapSlot, unchecked_return_type = "number | null")]
pub fn signer_swap_slot(
    #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
    #[wasm_bindgen(unchecked_param_type = "HexBytes")] payload: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        let vault = convert::vault(from_js(vault)?)?;
        let request =
            ome_protocol::payload::ExecutionRequest::from_payload(&convert::payload(payload)?)?;
        Ok(request
            .calls
            .iter()
            .filter(|call| membership::is_rotation_call(call, &vault))
            .find_map(|call| {
                let rotation = membership::Rotation::from_call(call, &vault).ok()?;
                membership::signer_swap_slot(&vault, &rotation).ok()
            }))
    })
}

/// The tagging secret for the vault's own unconstrained notes in committee epoch `epoch`, derived from that epoch's
/// ceremony transcript (the founding DKG, then each reshare), so a member removed earlier cannot derive it.
#[wasm_bindgen(js_name = epochTaggingSecret, unchecked_return_type = "Point")]
pub fn epoch_tagging_secret(
    #[wasm_bindgen(unchecked_param_type = "Hex32")] vault: &str,
    epoch: u32,
    #[wasm_bindgen(unchecked_param_type = "Hex64")] transcript: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        let transcript: Hex64 = transcript
            .parse()
            .map_err(|e: ome_api::HexError| WasmError::invalid(e.to_string()))?;
        let secret =
            membership::epoch_tagging_secret(convert::field(vault)?, epoch, &transcript.0)?;
        Ok(ome_api::Point::from_core(&secret))
    })
}

/// The newcomer a join request names, after checking that its keeper signed it. The member planning the rotation
/// still confirms out of band that it came from the person they mean to add.
#[wasm_bindgen(js_name = decodeJoinRequest, unchecked_return_type = "Joiner")]
pub fn decode_join_request(request: &str) -> Result<JsValue, JsValue> {
    export(|| {
        let member = ome_wire::messages::lobby::JoinRequest::decode(request)?.member;
        Ok(ome_api::daemon::Joiner {
            name: member.name,
            signer_key: ome_api::Point::from(&member.signer_key),
            keeper: ome_api::keeper::KeeperContact {
                identity: member.keeper.identity.0.into(),
                transport: member.keeper.transport.into(),
                addresses: member.keeper.addresses,
            },
        })
    })
}

/// The risks of spending threshold `k`, viewing threshold `t_v` and `n` members. None blocks a vault.
#[wasm_bindgen(js_name = policyWarnings, unchecked_return_type = "Warning[]")]
pub fn policy_warnings(
    threshold: u32,
    #[wasm_bindgen(js_name = viewingThreshold)] viewing_threshold: u32,
    members: u32,
) -> Result<JsValue, JsValue> {
    export(|| {
        Ok(policy::warnings(threshold, viewing_threshold, members)
            .into_iter()
            .map(|warning| match warning {
                policy::Warning::SingleSigner => Warning::SingleSigner,
                policy::Warning::NoSpareSigner => Warning::NoSpareSigner,
                policy::Warning::TwoCanRead => Warning::TwoCanRead,
                policy::Warning::NoSpareViewer => Warning::NoSpareViewer,
            })
            .collect::<Vec<_>>())
    })
}
