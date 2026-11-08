//! Values across the boundary: params arrive as plain JavaScript objects in the ome-api shapes and results leave the
//! same way (`0x` hex, `{ x, y }` points, numbers for counts and times, `null` for a missing value).

use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::JsValue;

use crate::error::{Result, WasmError};

/// Largest integer a JavaScript number holds exactly.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// Reads a param.
pub(crate) fn from_js<T: DeserializeOwned>(value: JsValue) -> Result<T> {
    serde_wasm_bindgen::from_value(value).map_err(|e| WasmError::invalid(e.to_string()))
}

/// Writes a result as plain objects and arrays (no `Map`s).
pub(crate) fn to_js<T: Serialize>(value: &T) -> Result<JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| WasmError::new(ome_api::ErrorKind::Failed, e.to_string()))
}

/// Runs an exported function: its result as a JavaScript value, its failure as a thrown `OmeWasmError`.
pub(crate) fn export<T: Serialize>(run: impl FnOnce() -> Result<T>) -> Result<JsValue, JsValue> {
    run().and_then(|value| to_js(&value)).map_err(JsValue::from)
}

/// Unix seconds from a JavaScript number.
pub(crate) fn seconds(value: f64) -> Result<u64> {
    if value.fract() != 0.0 || !(0.0..=MAX_SAFE_INTEGER).contains(&value) {
        return Err(WasmError::invalid(format!(
            "expected whole unix seconds, got {value}"
        )));
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked to be a whole number in the safe integer range"
    )]
    Ok(value as u64)
}
