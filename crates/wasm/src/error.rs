//! Errors thrown to JavaScript: an `Error` named `OmeWasmError` with the API error kind, so callers handle them the
//! way they handle the daemon's and keeper's.

use ome_api::{ErrorKind, core::InvalidValue};
use wasm_bindgen::JsValue;

/// A failure, before it becomes a JavaScript error.
#[derive(Debug)]
pub(crate) struct WasmError {
    kind: ErrorKind,
    message: String,
}

impl WasmError {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Invalid, message)
    }
}

impl std::fmt::Display for WasmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<WasmError> for JsValue {
    fn from(error: WasmError) -> Self {
        let js = js_sys::Error::new(&error.message);
        js.set_name("OmeWasmError");
        let kind = JsValue::from_str(error.kind.label());
        // Setting a property on a fresh Error object cannot fail.
        let _ = js_sys::Reflect::set(&js, &JsValue::from_str("kind"), &kind);
        js.into()
    }
}

impl From<InvalidValue> for WasmError {
    fn from(error: InvalidValue) -> Self {
        Self::invalid(error.0)
    }
}

impl From<ome_crypto::Error> for WasmError {
    fn from(error: ome_crypto::Error) -> Self {
        Self::invalid(error.to_string())
    }
}

impl From<ome_wire::Error> for WasmError {
    fn from(error: ome_wire::Error) -> Self {
        Self::invalid(error.to_string())
    }
}

impl From<ome_protocol::Error> for WasmError {
    fn from(error: ome_protocol::Error) -> Self {
        Self::invalid(error.to_string())
    }
}

impl From<ome_signer::Error> for WasmError {
    fn from(error: ome_signer::Error) -> Self {
        let kind = match error {
            ome_signer::Error::WrongSecret | ome_signer::Error::WrongSecretKind { .. } => {
                ErrorKind::Auth
            }
            ome_signer::Error::Kdf => ErrorKind::Failed,
            _ => ErrorKind::Invalid,
        };
        Self::new(kind, error.to_string())
    }
}

/// Result inside the bindings.
pub(crate) type Result<T, E = WasmError> = std::result::Result<T, E>;
