//! Errors every Ome API method can return, as a kind a client acts on plus a human message.

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What went wrong, in terms a client can act on. The CLI maps kinds to exit codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    /// The request line was not JSON.
    Parse,
    /// The request was not a JSON-RPC 2.0 request.
    InvalidRequest,
    /// No such method.
    MethodNotFound,
    /// The params, or a value inside them, were refused.
    Invalid,
    /// The session has not presented a valid token.
    Unauthorized,
    /// A vault, proposal, lobby or ceremony the request names is unknown.
    NotFound,
    /// The service is not ready for this yet: no profile, node unreachable, keeper starting.
    NotReady,
    /// The signer is locked; unlock it, or pass a secret with the request.
    Locked,
    /// The passphrase or key does not unseal the signer.
    Auth,
    /// The keeper refuses to act until a person looks: an unverifiable config or an unexpected spend.
    Alarm,
    /// Anything else.
    Failed,
}

impl ErrorKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 11] = [
        Self::Parse,
        Self::InvalidRequest,
        Self::MethodNotFound,
        Self::Invalid,
        Self::Unauthorized,
        Self::NotFound,
        Self::NotReady,
        Self::Locked,
        Self::Auth,
        Self::Alarm,
        Self::Failed,
    ];

    /// The JSON-RPC error code.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Parse => -32_700,
            Self::InvalidRequest => -32_600,
            Self::MethodNotFound => -32_601,
            Self::Invalid => -32_602,
            Self::Failed => -32_000,
            Self::Unauthorized => -32_001,
            Self::NotReady => -32_010,
            Self::Locked => -32_011,
            Self::Auth => -32_012,
            Self::Alarm => -32_013,
            Self::NotFound => -32_014,
        }
    }

    /// The kind with JSON-RPC error `code`; unknown codes are [`ErrorKind::Failed`].
    #[must_use]
    pub fn from_code(code: i32) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.code() == code)
            .unwrap_or(Self::Failed)
    }
}

/// The `data` member of an error response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ErrorData {
    /// What went wrong.
    pub kind: ErrorKind,
}

/// A JSON-RPC error object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RpcError {
    /// JSON-RPC code, [`ErrorKind::code`].
    pub code: i32,
    /// Human-readable, never containing a secret.
    pub message: String,
    /// The kind; absent only from a server that is not an Ome service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub data: Option<ErrorData>,
}

impl RpcError {
    /// An error of `kind`.
    #[must_use]
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            code: kind.code(),
            message: message.into(),
            data: Some(ErrorData { kind }),
        }
    }

    /// The kind, from `data` or else from the code.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.data
            .as_ref()
            .map_or_else(|| ErrorKind::from_code(self.code), |data| data.kind)
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RpcError {}
