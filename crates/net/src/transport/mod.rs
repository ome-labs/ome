//! The boundary a keeper depends on. It needs only "send these bytes to that member", a stream of inbound frames with
//! the channel that authenticated them, admission and reachability; no libp2p type crosses it.

mod libp2p;
mod memory;

use std::{future::Future, pin::Pin};

use ome_wire::{IdentityPublic, messages::ceremony::Member};

pub use self::libp2p::{Libp2pConfig, Libp2pTransport};
use crate::Error;
pub use memory::{MemoryNetwork, MemoryTransport};

/// A keeper to reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    /// Keeper identity (its libp2p peer id).
    pub identity: IdentityPublic,
    /// X25519 transport key.
    pub transport: [u8; 32],
    /// Network addresses (multiaddrs).
    pub addresses: Vec<String>,
}

impl From<&Member> for Peer {
    fn from(member: &Member) -> Self {
        Self {
            identity: member.identity,
            transport: member.transport,
            addresses: member.addresses.clone(),
        }
    }
}

/// What authenticated the channel a frame arrived on. The keeper requires the frame's signer to match it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// A connection authenticated as this keeper identity (libp2p's Noise handshake, or the in-process network).
    Peer(IdentityPublic),
    /// An envelope sealed with this X25519 transport key (relay mailboxes).
    Key([u8; 32]),
}

/// A frame received from a peer.
#[derive(Debug)]
pub struct Inbound {
    /// How the sender was authenticated.
    pub channel: Channel,
    /// Encoded `SignedMessage`.
    pub bytes: Vec<u8>,
}

/// What the transport knows about a peer's reachability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PeerStatus {
    /// There is an established connection to the peer.
    pub connected: bool,
    /// Every current connection is a relay circuit.
    pub relayed: bool,
    /// Unix seconds of the last established connection or received frame.
    pub last_seen: Option<u64>,
    /// Last measured round trip in milliseconds.
    pub rtt_ms: Option<u64>,
}

/// Boxed send future.
pub type SendFuture<'a> = Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>>;

/// Outbound side of a transport. Inbound frames arrive on the channel returned when the transport is created.
pub trait Transport: Send + Sync + 'static {
    /// Delivers `frame` to `peer`, best effort: protocols time out and retry.
    fn send(&self, peer: Peer, frame: Vec<u8>) -> SendFuture<'_>;

    /// Replaces the members this node talks to, and whether it hosts an open lobby (which admits strangers on the lobby
    /// lane). Connections the new admission refuses are closed.
    fn set_admission(&self, members: Vec<Peer>, lobby_open: bool);

    /// Reachability of each of `peers`, in order; unknown peers are reported unreachable.
    fn peer_status(&self, peers: &[IdentityPublic]) -> Vec<PeerStatus>;
}

/// Unix seconds now.
pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
