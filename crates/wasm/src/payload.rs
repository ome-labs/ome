//! Payload hashing, for checking a proposal independently of whoever built it.

use ome_api::Hex32;
use ome_protocol::payload::{self, ExecutionRequest};
use wasm_bindgen::prelude::*;

use crate::{
    convert,
    error::{Result, WasmError},
    js::{export, from_js},
    types::Call,
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
            extras: Vec::new(),
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

/// A function's selector from its signature, e.g. `transfer_private_to_private((Field),(Field),u128,Field)`.
#[wasm_bindgen]
pub fn selector(signature: &str) -> Result<u32, JsValue> {
    payload::selector(signature).map_err(|e| WasmError::from(e).into())
}
