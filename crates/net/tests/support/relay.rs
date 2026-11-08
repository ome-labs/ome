//! A test relay: the libp2p circuit relay server, identify, ping, and an in-memory mailbox store answering the mailbox
//! lane. `ome-relay` (phase 8) is the real one.

use std::sync::{Arc, Mutex};

use futures::StreamExt;
use libp2p::{
    Multiaddr, identify,
    multiaddr::Protocol,
    noise, ping, relay,
    request_response::{self, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use ome_net::{
    framing::{FrameCodec, IDENTIFY_VERSION, MAILBOX},
    identity::keypair,
};
use ome_wire::messages::mailbox::{
    MailboxRequest, MailboxResponse, StoredEnvelope, decode_request, encode_response,
};

use super::keeper;

#[derive(NetworkBehaviour)]
struct Behaviour {
    relay: relay::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    mailbox: request_response::Behaviour<FrameCodec>,
}

/// What a test relay stores.
pub type Store = Arc<Mutex<Vec<StoredEnvelope>>>;

/// Starts a relay with identity `seed` on loopback TCP; returns its address (ending in `/p2p/<id>`) and its store.
pub async fn spawn(seed: u8) -> (Multiaddr, Store) {
    let keys = keypair(&keeper(seed)).unwrap();
    let id = keys.public().to_peer_id();
    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keys)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|key| Behaviour {
            relay: relay::Behaviour::new(key.public().to_peer_id(), relay::Config::default()),
            identify: identify::Behaviour::new(identify::Config::new(
                IDENTIFY_VERSION.into(),
                key.public(),
            )),
            ping: ping::Behaviour::default(),
            mailbox: request_response::Behaviour::with_codec(
                FrameCodec::mailbox(),
                [(MAILBOX, ProtocolSupport::Inbound)],
                request_response::Config::default(),
            ),
        })
        .unwrap()
        .build();
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let listening = loop {
        if let Some(SwarmEvent::NewListenAddr { address, .. }) = swarm.next().await {
            break address;
        }
    };
    swarm.add_external_address(listening.clone());
    let store = Store::default();
    let shared = Arc::clone(&store);
    tokio::spawn(async move {
        loop {
            if let Some(SwarmEvent::Behaviour(BehaviourEvent::Mailbox(
                request_response::Event::Message {
                    message:
                        request_response::Message::Request {
                            request, channel, ..
                        },
                    ..
                },
            ))) = swarm.next().await
            {
                let response = answer(&shared, &request);
                let _ = swarm
                    .behaviour_mut()
                    .mailbox
                    .send_response(channel, encode_response(&response).unwrap());
            }
        }
    });
    (listening.with(Protocol::P2p(id)), store)
}

fn answer(store: &Store, request: &[u8]) -> MailboxResponse {
    let mut envelopes = store.lock().unwrap();
    match decode_request(request) {
        Ok(MailboxRequest::Deposit(envelope)) => {
            let id = envelope.id().unwrap();
            if envelopes.iter().all(|e| e.id != id) {
                envelopes.push(StoredEnvelope { id, envelope });
            }
            MailboxResponse::Stored { id }
        }
        Ok(MailboxRequest::Fetch { mailboxes }) => MailboxResponse::Envelopes(
            envelopes
                .iter()
                .filter(|e| mailboxes.contains(&e.envelope.recipient))
                .cloned()
                .collect(),
        ),
        Ok(MailboxRequest::Delete { ids }) => {
            let before = envelopes.len();
            envelopes.retain(|e| !ids.contains(&e.id));
            MailboxResponse::Deleted(u32::try_from(before - envelopes.len()).unwrap())
        }
        Err(_) => MailboxResponse::Refused(ome_wire::messages::mailbox::Refusal::Malformed),
    }
}
