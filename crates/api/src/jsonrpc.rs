//! JSON-RPC 2.0 envelopes and the typed method table both services are built from.
//!
//! Requests and responses are one JSON object per line. A session presents its token with `session.auth` before any
//! other call; after `events.subscribe` the server also writes notifications whose method is [`EVENT`].

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use ts_rs::{Config, TS};

use crate::RpcError;

/// The method of every event notification.
pub const EVENT: &str = "event";

/// A method: its name, what it takes and what it returns.
pub trait Method {
    /// Wire name, `area.verb`.
    const NAME: &'static str;
    /// Params object.
    type Params: Serialize + DeserializeOwned + TS + 'static;
    /// Result value.
    type Result: Serialize + DeserializeOwned + TS + 'static;
}

/// A method's name and TypeScript types, for the generated method maps.
#[derive(Clone, Copy)]
pub struct MethodInfo {
    /// Wire name.
    pub name: &'static str,
    /// TypeScript type of the params.
    pub params: fn(&Config) -> String,
    /// TypeScript type of the result.
    pub result: fn(&Config) -> String,
}

impl MethodInfo {
    /// The entry for `M`.
    #[must_use]
    pub const fn of<M: Method>() -> Self {
        Self {
            name: M::NAME,
            params: <M::Params as TS>::name,
            result: <M::Result as TS>::name,
        }
    }
}

/// Params of a method that takes none. Sent as `{}`; servers also accept a missing or `null` params member.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(type = "Record<string, never>")]
pub struct Empty {}

/// A request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Echoed in the response.
    #[serde(default)]
    pub id: Value,
    /// Method name.
    #[serde(default)]
    pub method: String,
    /// Params object; missing means none.
    #[serde(default)]
    pub params: Value,
}

impl Request {
    /// A request for `M`.
    ///
    /// # Errors
    /// Params that do not serialize.
    pub fn new<M: Method>(id: u64, params: &M::Params) -> serde_json::Result<Self> {
        Ok(Self {
            jsonrpc: "2.0".into(),
            id: id.into(),
            method: M::NAME.into(),
            params: serde_json::to_value(params)?,
        })
    }

    /// The params as `P`, reading a missing or `null` member as `{}`.
    ///
    /// # Errors
    /// Params that do not match `P`.
    pub fn params<P: DeserializeOwned>(&self) -> serde_json::Result<P> {
        match &self.params {
            Value::Null => serde_json::from_value(Value::Object(serde_json::Map::new())),
            params => P::deserialize(params),
        }
    }
}

/// A response: `result` or `error`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// The request's id (`null` when the request could not be read).
    pub id: Value,
    /// Result, on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error, on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// A success.
    #[must_use]
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A failure.
    #[must_use]
    pub fn err(id: Value, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// A server-to-client notification ([`EVENT`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notification<E> {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// [`EVENT`].
    pub method: String,
    /// The event.
    pub params: E,
}

impl<E> Notification<E> {
    /// An event notification.
    pub fn event(event: E) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: EVENT.into(),
            params: event,
        }
    }
}

/// Params of `session.auth`, the first call of every session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AuthParams {
    /// The per-launch token the service wrote next to its socket.
    pub token: String,
}

/// Declares method markers and the table of them.
macro_rules! methods {
    ($(#[$list_doc:meta])* $list:ident; $( $(#[$doc:meta])* $marker:ident = $name:literal ($params:ty) -> $result:ty; )*) => {
        $(
            $(#[$doc])*
            #[derive(Clone, Copy, Debug)]
            pub struct $marker;

            impl $crate::Method for $marker {
                const NAME: &'static str = $name;
                type Params = $params;
                type Result = $result;
            }
        )*

        $(#[$list_doc])*
        pub const $list: &[$crate::MethodInfo] = &[$($crate::MethodInfo::of::<$marker>()),*];
    };
}

pub(crate) use methods;
