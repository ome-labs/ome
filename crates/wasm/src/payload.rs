//! Payload hashing, for checking a proposal independently of whoever built it.

use ome_api::Hex32;
use ome_protocol::payload::{self, ExecutionRequest};
use wasm_bindgen::prelude::*;

use crate::{
    convert,
    error::{Result, WasmError},
    js::{export, from_js},
    types::{Authwit, Call},
};

fn calls(calls: JsValue) -> Result<Vec<payload::Call>> {
    Ok(from_js::<Vec<Call>>(calls)?
        .into_iter()
        .map(convert::call)
        .collect())
}

/// The outer hash the vault authorizes for `calls` with `txNonce` on `vault` and `chain`.
#[wasm_bindgen(js_name = outerHash, unchecked_return_type = "Hex32")]
pub fn outer_hash(
    #[wasm_bindgen(unchecked_param_type = "Hex32")] vault: &str,
    #[wasm_bindgen(unchecked_param_type = "Chain")] chain: JsValue,
    #[wasm_bindgen(js_name = txNonce, unchecked_param_type = "Hex32")] tx_nonce: &str,
    #[wasm_bindgen(unchecked_param_type = "Call[]")] calls: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let request = ExecutionRequest {
            calls: self::calls(calls)?,
            authwits: Vec::new(),
            extras: Vec::new(),
            extensions: ome_wire::Extensions::none(),
        };
        let chain = convert::chain(from_js(chain)?)?;
        Ok(Hex32::from_field(&request.outer_hash(
            convert::field(vault)?,
            chain,
            convert::field(tx_nonce)?,
        )?))
    })
}

/// The entrypoint's app payload hash (the authwit's inner hash).
#[wasm_bindgen(js_name = appPayloadHash, unchecked_return_type = "Hex32")]
pub fn app_payload_hash(
    #[wasm_bindgen(unchecked_param_type = "Call[]")] calls: JsValue,
    #[wasm_bindgen(js_name = txNonce, unchecked_param_type = "Hex32")] tx_nonce: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        Ok(Hex32::from_field(&payload::app_payload_hash(
            &self::calls(calls)?,
            convert::field(tx_nonce)?,
        )?))
    })
}

/// The message hash of a private authwit letting `caller` make `call` on the vault's behalf on `chain`, as aztec.js's
/// `computeAuthWitMessageHash({ caller, call })` computes it.
#[wasm_bindgen(js_name = authwitHash, unchecked_return_type = "Hex32")]
pub fn authwit_hash(
    #[wasm_bindgen(unchecked_param_type = "Authwit")] authwit: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Chain")] chain: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let authwit = convert::authwit(from_js::<Authwit>(authwit)?);
        Ok(Hex32::from_field(
            &authwit.message_hash(convert::chain(from_js(chain)?)?)?,
        ))
    })
}

/// A function's selector from its signature, e.g. `transfer_private_to_private((Field),(Field),u128,Field)`.
#[wasm_bindgen]
pub fn selector(signature: &str) -> Result<u32, JsValue> {
    payload::selector(signature).map_err(|e| WasmError::from(e).into())
}
