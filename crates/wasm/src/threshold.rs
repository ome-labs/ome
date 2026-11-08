//! Threshold reading: the PXE's view of the vault's shared incoming viewing key. The keepers answer an ECDH batch with
//! partials; these functions verify each partial's DLEQ proof and combine `t_v` of them into `address_secret · P`.

use ome_api::{Hex32, Point};
use ome_crypto::{
    Scalar, aztec,
    threshold::ecdh::{self, DleqProof, EcdhContext, Partial},
};
use ome_wire::types::parse_scalar;
use wasm_bindgen::prelude::*;

use crate::{
    convert,
    error::Result,
    js::{export, from_js},
    types::{AddressContext, CombineRequest, CombineResult},
};

/// Aztec's preaddress `Poseidon2([publicKeysHash, partialAddress], 993442748)`.
#[wasm_bindgen(unchecked_return_type = "Hex32")]
pub fn preaddress(
    #[wasm_bindgen(js_name = publicKeysHash, unchecked_param_type = "Hex32")]
    public_keys_hash: &str,
    #[wasm_bindgen(js_name = partialAddress, unchecked_param_type = "Hex32")] partial_address: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        Ok(Hex32::from_field(&aztec::preaddress(
            convert::field(public_keys_hash)?,
            convert::field(partial_address)?,
        )))
    })
}

fn context(preaddress: &str, ivpk: JsValue) -> Result<aztec::AddressContext> {
    let ivpk: Point = from_js(ivpk)?;
    Ok(aztec::AddressContext::new(
        convert::field(preaddress)?,
        &ivpk.to_core()?,
    )?)
}

/// The address context of a preaddress and group incoming viewing key: the vault address and the sign Aztec applies.
#[wasm_bindgen(js_name = addressContext, unchecked_return_type = "AddressContext")]
pub fn address_context(
    #[wasm_bindgen(unchecked_param_type = "Hex32")] preaddress: &str,
    #[wasm_bindgen(unchecked_param_type = "Point")] ivpk: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let context = context(preaddress, ivpk)?;
        Ok(AddressContext {
            address: Hex32::from_field(&context.address()),
            sign: if context.sign == Scalar::from(1u64) {
                1
            } else {
                -1
            },
            address_point: Point::from_core(&context.address_point),
        })
    })
}

/// Each member's public share of the address secret, `A_i`, from its verification share, in member order.
#[wasm_bindgen(js_name = addressPublicShares, unchecked_return_type = "Point[]")]
pub fn address_public_shares(
    #[wasm_bindgen(unchecked_param_type = "Hex32")] preaddress: &str,
    #[wasm_bindgen(unchecked_param_type = "Point")] ivpk: JsValue,
    #[wasm_bindgen(js_name = verificationShares, unchecked_param_type = "Point[]")]
    verification_shares: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let context = context(preaddress, ivpk)?;
        convert::points(from_js(verification_shares)?)?
            .iter()
            .map(|share| {
                Ok(Point::from_core(
                    &context.address_verification_share(share)?,
                ))
            })
            .collect::<Result<Vec<_>>>()
    })
}

/// Verifies every partial and combines `t_v` valid ones per input. Fails if an input lacks `t_v` valid partials.
#[wasm_bindgen(js_name = verifyAndCombine, unchecked_return_type = "CombineResult")]
pub fn verify_and_combine(
    #[wasm_bindgen(unchecked_param_type = "CombineRequest")] request: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| combine(from_js(request)?))
}

fn combine(request: CombineRequest) -> Result<CombineResult> {
    let context =
        aztec::AddressContext::new(request.preaddress.to_field()?, &request.ivpk.to_core()?)?;
    let ecdh_context = EcdhContext {
        vault: context.address(),
        epoch: request.epoch,
    };
    let publics = convert::points(request.verification_shares)?
        .iter()
        .map(|share| context.address_verification_share(share))
        .collect::<Result<Vec<_>, _>>()?;
    let inputs = convert::points(request.inputs)?;
    let threshold = usize::try_from(request.threshold).unwrap_or(usize::MAX);

    let mut invalid_members = Vec::new();
    let mut responses: Vec<Vec<Partial>> = Vec::new();
    for response in &request.responses {
        match decode(response, inputs.len()) {
            Some(partials) => responses.push(partials),
            None => invalid_members.push(response.member),
        }
    }
    let member_public = |index: u32| {
        publics
            .get(usize::try_from(index).ok()?.checked_sub(1)?)
            .copied()
    };
    let mut points = Vec::with_capacity(inputs.len());
    for (position, input) in inputs.iter().enumerate() {
        let partials: Vec<Partial> = responses.iter().map(|r| r[position]).collect();
        let (shared, rejected) =
            ecdh::verify_and_combine(&partials, input, member_public, threshold, &ecdh_context)?;
        for member in rejected {
            if !invalid_members.contains(&member) {
                invalid_members.push(member);
            }
        }
        points.push(Point::from_core(&shared));
    }
    invalid_members.sort_unstable();
    Ok(CombineResult {
        points,
        invalid_members,
    })
}

/// A member's partials, or `None` if any is malformed or they do not answer every input.
fn decode(response: &ome_api::keeper::MemberPartials, inputs: usize) -> Option<Vec<Partial>> {
    if response.partials.len() != inputs {
        return None;
    }
    response
        .partials
        .iter()
        .map(|p| {
            Some(Partial {
                index: response.member,
                point: p.point.to_core().ok()?,
                proof: DleqProof {
                    challenge: parse_scalar(&p.challenge.0).ok()?,
                    response: parse_scalar(&p.response.0).ok()?,
                },
            })
        })
        .collect()
}
