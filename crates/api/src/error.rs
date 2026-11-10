//! Errors every Ome API method can return, as a kind a client acts on plus a human message. A failure the member can
//! fix gets its own kind, with the numbers needed to fix it in [`ErrorDetails`]; `failed` is for bugs and unknowns.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use crate::ErrorDetails;

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
    /// The vault, or the fee payer, holds less than the transaction needs.
    InsufficientFunds,
    /// The proposal's deadline has passed.
    Expired,
    /// The proposal was made under a config the vault has since changed.
    Stale,
    /// Another member is already doing this (executing the same proposal).
    Conflict,
    /// The lobby has as many members as it takes.
    Full,
    /// Too few keepers answered to read the vault's private state.
    Unreadable,
    /// The contract is not registered with this member.
    UnknownContract,
    /// The invite, token or vault belongs to another network than this profile's.
    Network,
    /// Anything else, including a kind this version does not know.
    #[serde(other)]
    Failed,
}

impl ErrorKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 19] = [
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
        Self::InsufficientFunds,
        Self::Expired,
        Self::Stale,
        Self::Conflict,
        Self::Full,
        Self::Unreadable,
        Self::UnknownContract,
        Self::Network,
        Self::Failed,
    ];

    /// The kind's name on the wire.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::InvalidRequest => "invalid-request",
            Self::MethodNotFound => "method-not-found",
            Self::Invalid => "invalid",
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not-found",
            Self::NotReady => "not-ready",
            Self::Locked => "locked",
            Self::Auth => "auth",
            Self::Alarm => "alarm",
            Self::InsufficientFunds => "insufficient-funds",
            Self::Expired => "expired",
            Self::Stale => "stale",
            Self::Conflict => "conflict",
            Self::Full => "full",
            Self::Unreadable => "unreadable",
            Self::UnknownContract => "unknown-contract",
            Self::Network => "network",
            Self::Failed => "failed",
        }
    }

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
            Self::InsufficientFunds => -32_015,
            Self::Expired => -32_016,
            Self::Stale => -32_017,
            Self::Conflict => -32_018,
            Self::Full => -32_019,
            Self::Unreadable => -32_020,
            Self::UnknownContract => -32_021,
            Self::Network => -32_022,
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
    /// The numbers needed to fix it, for kinds that have them. Details a client doesn't understand read as absent.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_details"
    )]
    #[ts(optional)]
    pub details: Option<Box<ErrorDetails>>,
}

/// Details from a later version, or malformed ones, are dropped rather than failing the whole reply.
fn lenient_details<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Box<ErrorDetails>>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok().map(Box::new)))
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
            data: Some(ErrorData {
                kind,
                details: None,
            }),
        }
    }

    /// An error of the kind `details` belong to, carrying them.
    #[must_use]
    pub fn detailed(details: ErrorDetails, message: impl Into<String>) -> Self {
        let kind = details.kind();
        Self {
            code: kind.code(),
            message: message.into(),
            data: Some(ErrorData {
                kind,
                details: Some(Box::new(details)),
            }),
        }
    }

    /// The details, when the server sent ones this version understands.
    #[must_use]
    pub fn details(&self) -> Option<&ErrorDetails> {
        self.data.as_ref().and_then(|data| data.details.as_deref())
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
