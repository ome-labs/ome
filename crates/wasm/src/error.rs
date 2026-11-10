//! Errors thrown to JavaScript: an `Error` named `OmeWasmError` with the API error kind, and the kind's details when
//! it has them, so callers handle them the way they handle the daemon's and keeper's.

use ome_api::{ErrorDetails, ErrorKind, core::InvalidValue};
use wasm_bindgen::JsValue;

/// A failure, before it becomes a JavaScript error.
#[derive(Debug)]
pub(crate) struct WasmError {
    kind: ErrorKind,
    message: String,
    details: Option<Box<ErrorDetails>>,
}

impl WasmError {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            details: None,
        }
    }

    /// A failure of the kind `details` belong to.
    pub(crate) fn detailed(details: ErrorDetails, message: impl Into<String>) -> Self {
        Self {
            kind: details.kind(),
            message: message.into(),
            details: Some(Box::new(details)),
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
        if let Some(details) = error
            .details
            .and_then(|details| serde_wasm_bindgen::to_value(details.as_ref()).ok())
        {
            let _ = js_sys::Reflect::set(&js, &JsValue::from_str("details"), &details);
        }
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
        let message = error.to_string();
        match error {
            ome_protocol::Error::Expired { at } => {
                Self::detailed(ErrorDetails::Expired { at }, message)
            }
            ome_protocol::Error::StaleConfig { proposed, current } => Self::detailed(
                ErrorDetails::Stale {
                    made: proposed,
                    current,
                },
                message,
            ),
            _ => Self::invalid(message),
        }
    }
}

impl From<ome_signer::Error> for WasmError {
    fn from(error: ome_signer::Error) -> Self {
        // The signer runs the proposal checks itself; their failures read as the protocol's.
        if let ome_signer::Error::Protocol(protocol) = error {
            return protocol.into();
        }
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
