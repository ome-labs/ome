//! A real relay on loopback, and keeper nodes configured to use it.
// Each test binary uses a different subset of these helpers.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::missing_panics_doc
)]

use std::{collections::HashSet, time::Duration};

use libp2p::{Multiaddr, PeerId, multiaddr::Protocol};
use ome_net::{Libp2pConfig, identity::keypair};
use ome_relay::{Limits, Relay, RelayConfig, store::MailboxStore};
use ome_wire::{
    Header, KeeperIdentity, Message, SignedMessage, encode, messages::ceremony::Ceremony,
    messages::ceremony::Complaint, version::PROTOCOL_VERSION,
};

pub fn identity(n: u8) -> KeeperIdentity {
    KeeperIdentity::from_seed(&[n; 32])
}

pub fn peer_id(n: u8) -> PeerId {
    keypair(&identity(n)).unwrap().public().to_peer_id()
}

/// Starts a relay with identity `n` on loopback TCP; returns it and its dialable address.
pub async fn relay(n: u8, allow: Option<HashSet<PeerId>>) -> (Relay, Multiaddr) {
    let config = RelayConfig {
        listen: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        allow,
        ..RelayConfig::default()
    };
    let store = MailboxStore::in_memory(Limits::default()).unwrap();
    let relay = Relay::start(keypair(&identity(n)).unwrap(), store, config)
        .await
        .unwrap();
    let address = relay.addresses[0]
        .clone()
        .with(Protocol::P2p(relay.peer_id));
    (relay, address)
}

/// A keeper node config listening on `listen` only, using `relays`, with fast timeouts.
pub fn node(listen: &[&str], relays: Vec<Multiaddr>) -> Libp2pConfig {
    Libp2pConfig {
        listen: listen.iter().map(|a| a.parse().unwrap()).collect(),
        relays,
        external: Vec::new(),
        request_timeout: Duration::from_secs(2),
        ready_timeout: Duration::from_secs(5),
    }
}

/// A signed frame from keeper `from`.
pub fn frame(from: u8) -> Vec<u8> {
    let header = Header {
        scope: [1; 32],
        epoch: 0,
        id: [from; 16],
        created_at: 1_700_000_000,
    };
    let body = Message::Complaint(Complaint {
        ceremony: Ceremony::Dkg,
        accused: 2,
    });
    encode(&SignedMessage::sign(&identity(from), PROTOCOL_VERSION, header, body).unwrap()).unwrap()
}

/// An hour from now, within any relay's lifetime limit (the relay uses the real clock).
pub fn in_an_hour() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600
}
