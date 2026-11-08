//! The bytes Ome components exchange: keepers with keepers, a daemon with its own keeper, keepers with a relay.
//!
//! Every message travels as a [`SignedMessage`]: a protocol version, a [`Header`] (scope, epoch, id, timestamp) and a
//! [`Message`] body, signed by the sending keeper's Ed25519 identity over the canonical `postcard` encoding. Decoding
//! accepts only canonical bytes, enforces the [`limits`] and the [`version`] range, and checks per-message bounds.
//! [`ReplayGuard`] rejects duplicates, other epochs and stale timestamps. No transport lives here. See
//! `architecture/wire.md`.

pub mod codec;
mod error;
mod identity;
pub mod limits;
pub mod messages;
mod replay;
mod signed;
pub mod types;
pub mod version;

pub use codec::{decode, decode_body, encode, encode_body};
pub use error::Error;
pub use identity::{IdentityPublic, KeeperIdentity};
pub use messages::Message;
pub use replay::ReplayGuard;
pub use signed::{Header, SignedMessage};
