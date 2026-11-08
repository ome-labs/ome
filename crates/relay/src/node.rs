use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Swarm, allow_block_list, identify, identity, noise, ping, relay,
    request_response::{self, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent, behaviour::toggle::Toggle},
    tcp, yamux,
};
use ome_net::framing::{FrameCodec, IDENTIFY_VERSION, MAILBOX};
use ome_wire::messages::mailbox::{
    MailboxRequest, MailboxResponse, Refusal, decode_request, encode_response,
};
use tokio::sync::oneshot;

use crate::{Error, RelayConfig, Result, metrics::Metrics, store::MailboxStore};

#[derive(NetworkBehaviour)]
struct Behaviour {
    allow: Toggle<allow_block_list::Behaviour<allow_block_list::AllowedPeers>>,
    relay: relay::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    mailbox: request_response::Behaviour<FrameCodec>,
}

/// How long to keep collecting listen addresses after every listener has reported one.
const LISTEN_SETTLE: Duration = Duration::from_millis(150);

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn network_error(error: impl std::fmt::Display) -> Error {
    Error::Network(error.to_string())
}

/// A running relay.
pub struct Relay {
    /// Peer id.
    pub peer_id: PeerId,
    /// Bound listen addresses (append `/p2p/<peer id>` to dial).
    pub addresses: Vec<Multiaddr>,
    /// Envelope store.
    pub store: Arc<MailboxStore>,
    /// Counters.
    pub metrics: Arc<Metrics>,
    stop: Option<oneshot::Sender<()>>,
}

impl Relay {
    /// Starts the relay node.
    ///
    /// # Errors
    /// [`Error::Network`] if it cannot listen.
    pub async fn start(
        keypair: identity::Keypair,
        store: MailboxStore,
        config: RelayConfig,
    ) -> Result<Self> {
        let peer_id = keypair.public().to_peer_id();
        let allow = config.allow.as_ref().map(|peers| {
            let mut allow =
                allow_block_list::Behaviour::<allow_block_list::AllowedPeers>::default();
            for peer in peers {
                allow.allow_peer(*peer);
            }
            allow
        });
        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default().nodelay(true),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(network_error)?
            .with_quic()
            .with_dns()
            .map_err(network_error)?
            .with_behaviour(|key| Behaviour {
                allow: Toggle::from(allow),
                relay: relay::Behaviour::new(key.public().to_peer_id(), config.circuits.libp2p()),
                identify: identify::Behaviour::new(identify::Config::new(
                    IDENTIFY_VERSION.into(),
                    key.public(),
                )),
                ping: ping::Behaviour::default(),
                mailbox: request_response::Behaviour::with_codec(
                    FrameCodec::mailbox(),
                    [(MAILBOX, ProtocolSupport::Inbound)],
                    request_response::Config::default()
                        .with_request_timeout(Duration::from_secs(30)),
                ),
            })
            .map_err(network_error)?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_mins(5)))
            .build();
        for address in &config.listen {
            swarm.listen_on(address.clone()).map_err(network_error)?;
        }
        for address in &config.external {
            swarm.add_external_address(address.clone());
        }
        // A wildcard listener reports one address per interface: wait until every listener has reported, then keep
        // collecting briefly so every interface and both transports are announced.
        let mut addresses = Vec::new();
        let mut listeners = HashSet::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let mut settle: Option<tokio::time::Instant> = None;
        loop {
            if listeners.len() >= config.listen.len() {
                let until =
                    *settle.get_or_insert_with(|| tokio::time::Instant::now() + LISTEN_SETTLE);
                if tokio::time::Instant::now() >= until {
                    break;
                }
            }
            let wake = settle.map_or(deadline, |until| until.min(deadline));
            match tokio::time::timeout_at(wake, swarm.select_next_some()).await {
                Ok(SwarmEvent::NewListenAddr {
                    listener_id,
                    address,
                }) => {
                    if config.external.is_empty() {
                        swarm.add_external_address(address.clone());
                    }
                    listeners.insert(listener_id);
                    addresses.push(address);
                }
                Ok(_) => {}
                Err(_) if settle.is_some() => {}
                Err(_) => return Err(Error::Network("listeners did not come up".into())),
            }
        }
        let store = Arc::new(store);
        let metrics = Arc::new(Metrics::default());
        let (stop, stopped) = oneshot::channel();
        tokio::spawn(run(
            swarm,
            Arc::clone(&store),
            Arc::clone(&metrics),
            config,
            stopped,
        ));
        Ok(Self {
            peer_id,
            addresses,
            store,
            metrics,
            stop: Some(stop),
        })
    }

    /// Stops the relay.
    pub fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Most peers whose deposits are counted in one hour, so an open relay's table cannot be flooded.
const MAX_TRACKED_DEPOSITORS: usize = 65_536;

/// Deposits per peer in the current hour.
struct DepositBudget {
    per_hour: u32,
    hour: u64,
    used: HashMap<PeerId, u32>,
}

impl DepositBudget {
    fn take(&mut self, peer: PeerId, now: u64) -> bool {
        let hour = now / 3600;
        if hour != self.hour {
            self.hour = hour;
            self.used.clear();
        }
        if self.used.len() >= MAX_TRACKED_DEPOSITORS && !self.used.contains_key(&peer) {
            return false;
        }
        let used = self.used.entry(peer).or_default();
        *used += 1;
        *used <= self.per_hour
    }
}

async fn run(
    mut swarm: Swarm<Behaviour>,
    store: Arc<MailboxStore>,
    metrics: Arc<Metrics>,
    config: RelayConfig,
    mut stopped: oneshot::Receiver<()>,
) {
    let allowed: Option<HashSet<PeerId>> = config.allow.clone();
    let mut budget = DepositBudget {
        per_hour: config.limits.deposits_per_hour,
        hour: 0,
        used: HashMap::new(),
    };
    let mut purge = tokio::time::interval(Duration::from_mins(1));
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            _ = purge.tick() => {
                if let Err(error) = store.purge(now()) {
                    tracing::warn!(%error, "purge failed");
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(BehaviourEvent::Mailbox(request_response::Event::Message {
                    peer,
                    message: request_response::Message::Request { request, channel, .. },
                    ..
                })) => {
                    let allowed = allowed.as_ref().is_none_or(|a| a.contains(&peer));
                    let response = answer(&store, &metrics, &mut budget, peer, allowed, &request);
                    let bytes = encode_response(&response)
                        .or_else(|_| encode_response(&MailboxResponse::Refused(Refusal::Malformed)))
                        .unwrap_or_default();
                    let _ = swarm.behaviour_mut().mailbox.send_response(channel, bytes);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Relay(relay::Event::ReservationReqAccepted { src_peer_id, .. })) => {
                    Metrics::add(&metrics.reservations, 1);
                    tracing::debug!(peer = %src_peer_id, "reservation accepted");
                }
                SwarmEvent::Behaviour(BehaviourEvent::Relay(relay::Event::CircuitReqAccepted { .. })) => {
                    Metrics::add(&metrics.circuits, 1);
                    Metrics::add(&metrics.active_circuits, 1);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Relay(relay::Event::CircuitClosed { .. })) => {
                    metrics.circuit_closed();
                }
                _ => {}
            },
        }
    }
}

fn answer(
    store: &MailboxStore,
    metrics: &Metrics,
    budget: &mut DepositBudget,
    peer: PeerId,
    allowed: bool,
    request: &[u8],
) -> MailboxResponse {
    let reject = |reason| {
        metrics.refused(reason);
        MailboxResponse::Refused(reason)
    };
    let Ok(request) = decode_request(request) else {
        return reject(Refusal::Malformed);
    };
    if !allowed {
        return reject(Refusal::NotAllowed);
    }
    let now = now();
    let result = match request {
        MailboxRequest::Deposit(envelope) => {
            if !budget.take(peer, now) {
                return reject(Refusal::QuotaExceeded);
            }
            store.deposit(&envelope, now).map(|stored| match stored {
                Ok(id) => {
                    Metrics::add(&metrics.deposits, 1);
                    MailboxResponse::Stored { id }
                }
                Err(reason) => reject(reason),
            })
        }
        MailboxRequest::Fetch { mailboxes } => store.fetch(&mailboxes, now).map(|found| {
            Metrics::add(&metrics.fetches, 1);
            Metrics::add(&metrics.delivered, found.len() as u64);
            MailboxResponse::Envelopes(found)
        }),
        MailboxRequest::Delete { ids } => store.delete(&ids).map(|n| {
            Metrics::add(&metrics.deleted, u64::from(n));
            MailboxResponse::Deleted(n)
        }),
    };
    result.unwrap_or_else(|error| {
        tracing::error!(%error, "mailbox storage failed");
        reject(Refusal::Malformed)
    })
}
