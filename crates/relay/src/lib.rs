//! Ome relay: untrusted infrastructure that helps keepers reach each other and holds ciphertext for members who are
//! offline. It runs a libp2p circuit relay v2 server (and the rendezvous for hole punching) and a mailbox of sealed
//! envelopes under opaque ids. It never sees a plaintext, an identity tied to a vault, or a vault address; it does see
//! which peers connect and when. See `architecture/networking.md`.

mod error;
pub mod health;
pub mod metrics;
mod node;
pub mod store;

use std::{collections::HashSet, time::Duration};

pub use error::{Error, Result};
use libp2p::{Multiaddr, PeerId};
pub use node::Relay;

/// Storage and rate limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Longest accepted envelope lifetime.
    pub max_ttl_secs: u64,
    /// Most envelopes per mailbox.
    pub max_envelopes_per_mailbox: u64,
    /// Most bytes per mailbox.
    pub max_bytes_per_mailbox: u64,
    /// Most bytes held in total.
    pub max_total_bytes: u64,
    /// Most deposits per peer per hour.
    pub deposits_per_hour: u32,
    /// Most bytes returned by one fetch.
    pub max_fetch_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_ttl_secs: 14 * 86_400,
            max_envelopes_per_mailbox: 512,
            max_bytes_per_mailbox: 32 << 20,
            max_total_bytes: 1 << 30,
            deposits_per_hour: 3_600,
            max_fetch_bytes: 3 << 20,
        }
    }
}

/// Circuit relay limits. Keepers fall back to relayed circuits when hole punching fails, so circuits must carry a
/// ceremony or an ECDH batch (libp2p's defaults of 2 minutes and 128 KiB are too small).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CircuitLimits {
    /// Reservations held at once.
    pub max_reservations: usize,
    /// Circuits relayed at once.
    pub max_circuits: usize,
    /// Circuits per peer.
    pub max_circuits_per_peer: usize,
    /// Lifetime of one circuit.
    pub max_circuit_duration: Duration,
    /// Bytes through one circuit.
    pub max_circuit_bytes: u64,
}

impl Default for CircuitLimits {
    fn default() -> Self {
        Self {
            max_reservations: 1024,
            max_circuits: 256,
            max_circuits_per_peer: 16,
            max_circuit_duration: Duration::from_mins(30),
            max_circuit_bytes: 64 << 20,
        }
    }
}

impl CircuitLimits {
    pub(crate) fn libp2p(self) -> libp2p::relay::Config {
        libp2p::relay::Config {
            max_reservations: self.max_reservations,
            max_circuits: self.max_circuits,
            max_circuits_per_peer: self.max_circuits_per_peer,
            max_circuit_duration: self.max_circuit_duration,
            max_circuit_bytes: self.max_circuit_bytes,
            ..libp2p::relay::Config::default()
        }
    }
}

/// Relay settings.
#[derive(Clone, Debug, Default)]
pub struct RelayConfig {
    /// Listen addresses.
    pub listen: Vec<Multiaddr>,
    /// Public addresses to announce (default: the listen addresses).
    pub external: Vec<Multiaddr>,
    /// When set, only these peers may reserve circuits or use mailboxes.
    pub allow: Option<HashSet<PeerId>>,
    /// Mailbox limits.
    pub limits: Limits,
    /// Circuit relay limits.
    pub circuits: CircuitLimits,
}
