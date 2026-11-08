//! Shared by the networking tests: signed frames for each lane and keeper identities.
// Each test binary uses a different subset of these helpers.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::missing_panics_doc
)]

use std::time::Duration;

use libp2p::Multiaddr;
use ome_net::{Inbound, Libp2pConfig, Peer};
use ome_wire::{
    Header, KeeperIdentity, Message, SignedMessage, encode,
    messages::{
        ceremony::{Ceremony, Complaint},
        ecdh::{EcdhPurpose, EcdhRequest},
        lobby::LobbyLeave,
        proposals::ExecutionReceipt,
    },
    types::WirePoint,
    version::PROTOCOL_VERSION,
};
use tokio::sync::mpsc;

pub fn keeper(n: u8) -> KeeperIdentity {
    KeeperIdentity::from_seed(&[n; 32])
}

pub fn peer(n: u8, addresses: &[String]) -> Peer {
    Peer {
        identity: keeper(n).public(),
        transport: [n; 32],
        addresses: addresses.to_vec(),
    }
}

fn frame(from: u8, body: Message) -> Vec<u8> {
    let header = Header {
        scope: [1; 32],
        epoch: 0,
        id: [from; 16],
        created_at: 1_700_000_000,
    };
    encode(&SignedMessage::sign(&keeper(from), PROTOCOL_VERSION, header, body).unwrap()).unwrap()
}

pub fn ecdh_frame(from: u8) -> Vec<u8> {
    // Decoding checks bounds, not curve membership, so any coordinates make a valid frame.
    let point = WirePoint {
        x: [1; 32],
        y: [2; 32],
    };
    frame(
        from,
        Message::EcdhRequest(EcdhRequest {
            purpose: EcdhPurpose::NoteDecryption,
            points: vec![point],
        }),
    )
}

pub fn ceremony_frame(from: u8) -> Vec<u8> {
    frame(
        from,
        Message::Complaint(Complaint {
            ceremony: Ceremony::Dkg,
            accused: 2,
        }),
    )
}

pub fn sync_frame(from: u8) -> Vec<u8> {
    frame(
        from,
        Message::ExecutionReceipt(ExecutionReceipt {
            proposal: [3; 32],
            tx_hash: [4; 32],
        }),
    )
}

pub fn lobby_frame(from: u8) -> Vec<u8> {
    frame(from, Message::LobbyLeave(LobbyLeave { lobby: [5; 16] }))
}

pub fn address(text: &str) -> Multiaddr {
    text.parse().unwrap()
}

/// A node config listening on `listen` only, with fast timeouts.
pub fn config(listen: &[&str], relays: Vec<Multiaddr>) -> Libp2pConfig {
    Libp2pConfig {
        listen: listen.iter().map(|a| address(a)).collect(),
        relays,
        external: Vec::new(),
        request_timeout: Duration::from_secs(2),
        ready_timeout: Duration::from_secs(5),
    }
}

/// The next inbound frame, or `None` after `wait`.
pub async fn next(
    inbound: &mut mpsc::UnboundedReceiver<Inbound>,
    wait: Duration,
) -> Option<Inbound> {
    tokio::time::timeout(wait, inbound.recv())
        .await
        .ok()
        .flatten()
}
