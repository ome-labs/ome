//! How Ome keepers, relays and clients reach each other.
//!
//! - [`Transport`] is the boundary a keeper depends on: send a frame to a member, receive frames with the channel that
//!   authenticated them, set who may connect, read reachability. [`MemoryNetwork`] implements it in process;
//!   [`Libp2pTransport`] over QUIC or TCP with Noise, relay circuits and hole punching.
//! - [`framing`] holds the stream protocols (one per conversation, versioned by name) and the length-prefixed codec.
//! - [`Admission`] decides who may connect and which lane a stranger may use while a lobby is open.
//! - [`Mailbox`] stores sealed envelopes for offline members, at relays ([`RelayMailbox`]) or in memory.
//!
//! Frames are always `ome-wire` signed messages, so authenticity never depends on the transport: the keeper checks
//! that a frame's signer is the peer its channel authenticated. Native only. See `architecture/networking.md`.

mod error;
pub mod framing;
mod gate;
pub mod identity;
mod mailbox;
mod transport;

pub use error::Error;
pub use gate::{Admission, LOBBY_REQUESTS_PER_MINUTE};
pub use mailbox::{Mailbox, MailboxFuture, MemoryMailbox, RelayMailbox};
pub use transport::{
    Channel, Inbound, Libp2pConfig, Libp2pTransport, MemoryNetwork, MemoryTransport, Peer,
    PeerStatus, SendFuture, Transport,
};
