//! The libp2p node: QUIC, or TCP with Noise and Yamux; DNS; relay-client reservations so a keeper behind NAT stays
//! dialable through a circuit, and `DCUtR` to upgrade circuits to direct connections; identify and ping. The peer id is
//! the keeper identity, so a connection authenticates the member.

use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    task::{Context, Poll},
    time::Duration,
};

use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Swarm,
    core::{Endpoint, transport::PortUse},
    dcutr, identify,
    multiaddr::Protocol,
    noise, ping, relay,
    request_response::{self, OutboundRequestId, ProtocolSupport},
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, SwarmEvent, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm, dummy,
    },
    tcp, yamux,
};
use ome_wire::{IdentityPublic, KeeperIdentity, limits::MAX_ADDRESSES};
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};

use super::{Channel, Inbound, Peer, PeerStatus, SendFuture, Transport, unix_now};
use crate::{
    Admission, Error,
    error::transport_error,
    framing::{CEREMONY, ECDH, FrameCodec, IDENTIFY_VERSION, LOBBY, Lane, MAILBOX, SYNC},
    identity::{identity_of, keypair, peer_id},
};

/// How long to keep collecting listen addresses after every listener has reported one.
const LISTEN_SETTLE: Duration = Duration::from_millis(150);
/// How long an idle connection stays open.
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Settings of a node.
#[derive(Clone, Debug)]
pub struct Libp2pConfig {
    /// Listen addresses, e.g. `/ip4/0.0.0.0/udp/0/quic-v1` and `/ip4/0.0.0.0/tcp/0`.
    pub listen: Vec<Multiaddr>,
    /// Relays, each ending in `/p2p/<relay peer id>`: the node reserves a circuit at each and uses them as mailboxes.
    pub relays: Vec<Multiaddr>,
    /// Extra addresses to advertise, such as a public address behind port forwarding.
    pub external: Vec<Multiaddr>,
    /// Timeout of one request.
    pub request_timeout: Duration,
    /// How long [`Libp2pTransport::start`] waits for listeners and relay reservations.
    pub ready_timeout: Duration,
    /// Whether to reserve a circuit at each relay so peers can dial this node. A client that only uses the relays'
    /// mailboxes (a signing device) reserves none and listens on nothing.
    pub reserve: bool,
}

impl Default for Libp2pConfig {
    fn default() -> Self {
        let local = |address: &str| address.parse().unwrap_or_else(|_| Multiaddr::empty());
        Self {
            listen: vec![
                local("/ip4/127.0.0.1/udp/0/quic-v1"),
                local("/ip4/127.0.0.1/tcp/0"),
            ],
            relays: Vec::new(),
            external: Vec::new(),
            request_timeout: Duration::from_secs(10),
            ready_timeout: Duration::from_secs(10),
            reserve: true,
        }
    }
}

/// Refuses connections [`Admission`] does not admit, before any protocol runs.
#[derive(Default)]
struct Gate {
    admission: Admission,
}

#[derive(Debug)]
struct NotAdmitted(PeerId);

impl std::fmt::Display for NotAdmitted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "peer {} is not admitted", self.0)
    }
}

impl std::error::Error for NotAdmitted {}

impl Gate {
    fn admits(&self, peer: &PeerId) -> bool {
        identity_of(peer).is_some_and(|identity| self.admission.admits(&identity))
    }

    fn enforce(&self, peer: &PeerId) -> Result<(), ConnectionDenied> {
        if self.admits(peer) {
            Ok(())
        } else {
            Err(ConnectionDenied::new(NotAdmitted(*peer)))
        }
    }
}

impl NetworkBehaviour for Gate {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.enforce(&peer)?;
        Ok(dummy::ConnectionHandler)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer: Option<PeerId>,
        _: &[Multiaddr],
        _: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        if let Some(peer) = peer {
            self.enforce(&peer)?;
        }
        Ok(Vec::new())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.enforce(&peer)?;
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, _: FromSwarm) {}

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {}
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Infallible, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    gate: Gate,
    relay_client: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    ecdh: request_response::Behaviour<FrameCodec>,
    ceremony: request_response::Behaviour<FrameCodec>,
    sync: request_response::Behaviour<FrameCodec>,
    lobby: request_response::Behaviour<FrameCodec>,
    mailbox: request_response::Behaviour<FrameCodec>,
}

/// What the node observed of one peer's connections.
#[derive(Default)]
struct Link {
    /// Open connections and whether each is a relay circuit.
    connections: HashMap<ConnectionId, bool>,
    last_seen: Option<u64>,
    rtt_ms: Option<u64>,
}

/// Links by peer, written by the swarm task and read by [`Transport::peer_status`].
type Links = Arc<Mutex<HashMap<PeerId, Link>>>;

enum Command {
    Send {
        peer: Peer,
        frame: Vec<u8>,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    Mailbox {
        relay: PeerId,
        request: Vec<u8>,
        reply: oneshot::Sender<Result<Vec<u8>, Error>>,
    },
    Admit {
        members: Vec<Peer>,
        lobby_open: bool,
    },
    Stop,
}

/// A running libp2p node. Dropping it does not stop the node; call [`Self::stop`].
pub struct Libp2pTransport {
    commands: mpsc::UnboundedSender<Command>,
    links: Links,
    peer_id: PeerId,
    relays: Vec<PeerId>,
    addresses: Vec<String>,
}

impl Libp2pTransport {
    /// Starts a node as `keeper`. Returns once every listener is bound and every relay accepted a reservation, or the
    /// ready timeout passed, with the inbound frame stream.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a relay address without a peer id, [`Error::Transport`] if the node cannot start.
    pub async fn start(
        keeper: &KeeperIdentity,
        config: Libp2pConfig,
    ) -> Result<(Arc<Self>, mpsc::UnboundedReceiver<Inbound>), Error> {
        let keypair = keypair(keeper)?;
        let local = keypair.public().to_peer_id();
        let timeout = config.request_timeout;
        let lane = |protocol| {
            request_response::Behaviour::with_codec(
                FrameCodec::keeper(),
                [(protocol, ProtocolSupport::Full)],
                request_response::Config::default().with_request_timeout(timeout),
            )
        };
        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default().nodelay(true),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(transport_error)?
            .with_quic()
            .with_dns()
            .map_err(transport_error)?
            .with_relay_client(noise::Config::new, yamux::Config::default)
            .map_err(transport_error)?
            .with_behaviour(|key, relay_client| Behaviour {
                gate: Gate::default(),
                relay_client,
                dcutr: dcutr::Behaviour::new(key.public().to_peer_id()),
                identify: identify::Behaviour::new(identify::Config::new(
                    IDENTIFY_VERSION.into(),
                    key.public(),
                )),
                ping: ping::Behaviour::default(),
                ecdh: lane(ECDH),
                ceremony: lane(CEREMONY),
                sync: lane(SYNC),
                lobby: lane(LOBBY),
                mailbox: request_response::Behaviour::with_codec(
                    FrameCodec::mailbox(),
                    [(MAILBOX, ProtocolSupport::Outbound)],
                    request_response::Config::default().with_request_timeout(timeout * 3),
                ),
            })
            .map_err(transport_error)?
            .with_swarm_config(|c| c.with_idle_connection_timeout(IDLE_TIMEOUT))
            .build();

        let mut relays = Vec::new();
        for relay in &config.relays {
            let Some(Protocol::P2p(id)) = relay.iter().last() else {
                return Err(Error::Invalid(format!(
                    "relay address {relay} must end with /p2p/<peer id>"
                )));
            };
            let identity = identity_of(&id)
                .ok_or_else(|| Error::Invalid(format!("relay {id} is not an Ed25519 key")))?;
            swarm.behaviour_mut().gate.admission.add_relay(identity);
            swarm.add_peer_address(id, relay.clone());
            relays.push((id, relay.clone()));
        }
        for address in &config.listen {
            swarm.listen_on(address.clone()).map_err(transport_error)?;
        }
        if config.reserve {
            for (_, relay) in &relays {
                swarm
                    .listen_on(relay.clone().with(Protocol::P2pCircuit))
                    .map_err(transport_error)?;
            }
        }

        let (listening, reserved) = wait_ready(&mut swarm, &config, &relays).await;
        let addresses = advertised(&config, &relays, &listening, &reserved);
        tracing::info!(peer = %local, ?addresses, "node ready");

        let (commands, command_rx) = mpsc::unbounded_channel();
        let (inbound, inbound_rx) = mpsc::unbounded_channel();
        let relay_ids: Vec<PeerId> = relays.iter().map(|(id, _)| *id).collect();
        let links = Links::default();
        tokio::spawn(run(
            swarm,
            command_rx,
            inbound,
            relay_ids.iter().copied().collect(),
            Arc::clone(&links),
        ));
        Ok((
            Arc::new(Self {
                commands,
                links,
                peer_id: local,
                relays: relay_ids,
                addresses,
            }),
            inbound_rx,
        ))
    }

    /// This node's peer id.
    #[must_use]
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Addresses to publish in manifests: direct addresses first, then relay circuits; at most eight.
    #[must_use]
    pub fn addresses(&self) -> &[String] {
        &self.addresses
    }

    /// Configured relays.
    #[must_use]
    pub fn relays(&self) -> &[PeerId] {
        &self.relays
    }

    /// Stops the node: closes every connection and ends the inbound stream.
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    /// Sends a mailbox request to `relay` and returns its raw answer.
    pub(crate) async fn mailbox(&self, relay: PeerId, request: Vec<u8>) -> Result<Vec<u8>, Error> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Mailbox {
                relay,
                request,
                reply,
            })
            .map_err(|_| Error::Stopped)?;
        answer.await.map_err(|_| Error::Stopped)?
    }
}

impl Transport for Arc<Libp2pTransport> {
    fn send(&self, peer: Peer, frame: Vec<u8>) -> SendFuture<'_> {
        Box::pin(async move {
            let (reply, answer) = oneshot::channel();
            self.commands
                .send(Command::Send { peer, frame, reply })
                .map_err(|_| Error::Stopped)?;
            answer.await.map_err(|_| Error::Stopped)?
        })
    }

    fn set_admission(&self, members: Vec<Peer>, lobby_open: bool) {
        let _ = self.commands.send(Command::Admit {
            members,
            lobby_open,
        });
    }

    fn peer_status(&self, peers: &[IdentityPublic]) -> Vec<PeerStatus> {
        let links = lock(&self.links);
        peers
            .iter()
            .map(|identity| {
                let Some(link) = peer_id(identity).ok().and_then(|id| links.get(&id)) else {
                    return PeerStatus::default();
                };
                let connected = !link.connections.is_empty();
                PeerStatus {
                    connected,
                    relayed: connected && link.connections.values().all(|relayed| *relayed),
                    last_seen: link.last_seen,
                    rtt_ms: link.rtt_ms,
                }
            })
            .collect()
    }
}

/// Addresses to advertise: direct addresses first (peers try them before relaying), then relay circuits, which are
/// always included as the fallback.
fn advertised(
    config: &Libp2pConfig,
    relays: &[(PeerId, Multiaddr)],
    listening: &[Multiaddr],
    reserved: &HashSet<PeerId>,
) -> Vec<String> {
    let circuits: Vec<String> = relays
        .iter()
        .filter(|(id, _)| reserved.contains(id))
        .map(|(_, relay)| relay.clone().with(Protocol::P2pCircuit).to_string())
        .take(MAX_ADDRESSES)
        .collect();
    // A wildcard listener reports loopback too. Advertised to another host, loopback would dial the dialer itself, and
    // libp2p settles a dial on its first transport success, so that self-connection would fail the whole dial. Only a
    // loopback-only node advertises loopback.
    let routable = listening.iter().any(|a| !is_loopback(a));
    let mut addresses: Vec<String> = config
        .external
        .iter()
        .chain(listening.iter().filter(|a| !(routable && is_loopback(a))))
        .map(ToString::to_string)
        .collect();
    addresses.dedup();
    addresses.truncate(MAX_ADDRESSES - circuits.len());
    addresses.extend(circuits);
    addresses
}

/// Records a peer's advertised addresses, skipping our own: a member behind another NAT may advertise the same private
/// address and port, and dialing it would reach this node.
fn add_addresses(swarm: &mut Swarm<Behaviour>, id: PeerId, addresses: &[String]) {
    for address in addresses.iter().filter_map(|a| a.parse::<Multiaddr>().ok()) {
        if !swarm.listeners().any(|own| *own == address) {
            swarm.add_peer_address(id, address);
        }
    }
}

fn is_loopback(address: &Multiaddr) -> bool {
    address.iter().any(|p| match p {
        Protocol::Ip4(ip) => ip.is_loopback(),
        Protocol::Ip6(ip) => ip.is_loopback(),
        _ => false,
    })
}

/// Waits until every listen address is bound and every relay accepted a reservation, or the timeout passes.
async fn wait_ready(
    swarm: &mut Swarm<Behaviour>,
    config: &Libp2pConfig,
    relays: &[(PeerId, Multiaddr)],
) -> (Vec<Multiaddr>, HashSet<PeerId>) {
    let deadline = Instant::now() + config.ready_timeout;
    let mut listening = Vec::new();
    let mut listeners = HashSet::new();
    let mut reserved = HashSet::new();
    // A wildcard listener reports one address per interface; once every listener has reported, keep collecting
    // briefly so every interface and both transports make it into what is advertised.
    let mut settle: Option<Instant> = None;
    loop {
        let reservations = if config.reserve { relays.len() } else { 0 };
        if listeners.len() >= config.listen.len() && reserved.len() >= reservations {
            let until = *settle.get_or_insert_with(|| Instant::now() + LISTEN_SETTLE);
            if Instant::now() >= until {
                break;
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        let wake = settle.map_or(deadline, |until| until.min(deadline));
        let Ok(Some(event)) = tokio::time::timeout_at(wake, swarm.next()).await else {
            continue;
        };
        match event {
            SwarmEvent::NewListenAddr {
                listener_id,
                address,
            } if !address.iter().any(|p| p == Protocol::P2pCircuit) => {
                listeners.insert(listener_id);
                listening.push(address);
            }
            SwarmEvent::Behaviour(BehaviourEvent::RelayClient(
                relay::client::Event::ReservationReqAccepted { relay_peer_id, .. },
            )) => {
                reserved.insert(relay_peer_id);
            }
            _ => {}
        }
    }
    (listening, reserved)
}

/// Requests in flight and the link table.
struct Pending {
    sends: HashMap<OutboundRequestId, oneshot::Sender<Result<(), Error>>>,
    mailbox: HashMap<OutboundRequestId, oneshot::Sender<Result<Vec<u8>, Error>>>,
    links: Links,
}

impl Pending {
    fn seen(&self, peer: &PeerId) {
        if let Some(link) = lock(&self.links).get_mut(peer) {
            link.last_seen = Some(unix_now());
        }
    }
}

async fn run(
    mut swarm: Swarm<Behaviour>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    inbound: mpsc::UnboundedSender<Inbound>,
    relays: HashSet<PeerId>,
    links: Links,
) {
    let mut pending = Pending {
        sends: HashMap::new(),
        mailbox: HashMap::new(),
        links,
    };
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Stop) | None => break,
                Some(command) => on_command(&mut swarm, &mut pending, &relays, command),
            },
            Some(event) = swarm.next() => on_event(&mut swarm, &mut pending, &inbound, event),
        }
    }
}

fn on_command(
    swarm: &mut Swarm<Behaviour>,
    pending: &mut Pending,
    relays: &HashSet<PeerId>,
    command: Command,
) {
    match command {
        Command::Send { peer, frame, reply } => {
            if !swarm.behaviour().gate.admission.admits(&peer.identity) {
                let _ = reply.send(Err(Error::NotAdmitted));
                return;
            }
            let id = match peer_id(&peer.identity) {
                Ok(id) => id,
                Err(error) => {
                    let _ = reply.send(Err(error));
                    return;
                }
            };
            add_addresses(swarm, id, &peer.addresses);
            let behaviour = swarm.behaviour_mut();
            let request = match Lane::of(&frame) {
                Lane::Ecdh => behaviour.ecdh.send_request(&id, frame),
                Lane::Ceremony => behaviour.ceremony.send_request(&id, frame),
                Lane::Sync => behaviour.sync.send_request(&id, frame),
                Lane::Lobby => behaviour.lobby.send_request(&id, frame),
            };
            pending.sends.insert(request, reply);
        }
        Command::Mailbox {
            relay,
            request,
            reply,
        } => {
            let request = swarm.behaviour_mut().mailbox.send_request(&relay, request);
            pending.mailbox.insert(request, reply);
        }
        Command::Admit {
            members,
            lobby_open,
        } => {
            for peer in &members {
                if let Ok(id) = peer_id(&peer.identity) {
                    add_addresses(swarm, id, &peer.addresses);
                }
            }
            swarm
                .behaviour_mut()
                .gate
                .admission
                .set(members.iter().map(|p| p.identity), lobby_open);
            // Close what the gate no longer admits: removed members, and strangers once no lobby is open.
            let closing: Vec<PeerId> = swarm
                .connected_peers()
                .filter(|id| !relays.contains(id) && !swarm.behaviour().gate.admits(id))
                .copied()
                .collect();
            for id in closing {
                let _ = swarm.disconnect_peer_id(id);
            }
            let gate = &swarm.behaviour().gate;
            lock(&pending.links).retain(|id, link| !link.connections.is_empty() || gate.admits(id));
        }
        Command::Stop => {}
    }
}

fn on_event(
    swarm: &mut Swarm<Behaviour>,
    pending: &mut Pending,
    inbound: &mpsc::UnboundedSender<Inbound>,
    event: SwarmEvent<BehaviourEvent>,
) {
    match event {
        SwarmEvent::Behaviour(BehaviourEvent::Ecdh(event)) => {
            on_lane(swarm, pending, inbound, Lane::Ecdh, event);
        }
        SwarmEvent::Behaviour(BehaviourEvent::Ceremony(event)) => {
            on_lane(swarm, pending, inbound, Lane::Ceremony, event);
        }
        SwarmEvent::Behaviour(BehaviourEvent::Sync(event)) => {
            on_lane(swarm, pending, inbound, Lane::Sync, event);
        }
        SwarmEvent::Behaviour(BehaviourEvent::Lobby(event)) => {
            on_lane(swarm, pending, inbound, Lane::Lobby, event);
        }
        SwarmEvent::Behaviour(BehaviourEvent::Mailbox(event)) => match event {
            request_response::Event::Message {
                message:
                    request_response::Message::Response {
                        request_id,
                        response,
                    },
                ..
            } => {
                if let Some(reply) = pending.mailbox.remove(&request_id) {
                    let _ = reply.send(Ok(response));
                }
            }
            request_response::Event::OutboundFailure {
                request_id, error, ..
            } => {
                if let Some(reply) = pending.mailbox.remove(&request_id) {
                    let _ = reply.send(Err(transport_error(error)));
                }
            }
            _ => {}
        },
        SwarmEvent::Behaviour(BehaviourEvent::Dcutr(event)) => {
            tracing::debug!(?event, "hole punching");
        }
        SwarmEvent::Behaviour(BehaviourEvent::Ping(ping::Event {
            peer,
            result: Ok(rtt),
            ..
        })) => {
            if let Some(link) = lock(&pending.links).get_mut(&peer) {
                link.rtt_ms = Some(u64::try_from(rtt.as_millis()).unwrap_or(u64::MAX));
            }
        }
        SwarmEvent::ConnectionEstablished {
            peer_id,
            connection_id,
            endpoint,
            ..
        } => {
            tracing::debug!(%peer_id, relayed = endpoint.is_relayed(), "connected");
            let mut links = lock(&pending.links);
            let link = links.entry(peer_id).or_default();
            link.connections
                .insert(connection_id, endpoint.is_relayed());
            link.last_seen = Some(unix_now());
        }
        SwarmEvent::ConnectionClosed {
            peer_id,
            connection_id,
            ..
        } => {
            let gate = &swarm.behaviour().gate;
            let mut links = lock(&pending.links);
            if let Some(link) = links.get_mut(&peer_id) {
                link.connections.remove(&connection_id);
            }
            links.retain(|id, link| !link.connections.is_empty() || gate.admits(id));
        }
        _ => {}
    }
}

/// One keeper lane's events: deliver admitted requests and acknowledge them, settle our own requests.
fn on_lane(
    swarm: &mut Swarm<Behaviour>,
    pending: &mut Pending,
    inbound: &mpsc::UnboundedSender<Inbound>,
    lane: Lane,
    event: request_response::Event<Vec<u8>, Vec<u8>>,
) {
    match event {
        request_response::Event::Message {
            peer,
            message:
                request_response::Message::Request {
                    request, channel, ..
                },
            ..
        } => {
            pending.seen(&peer);
            let Some(identity) = identity_of(&peer) else {
                return;
            };
            // The frame's own lane must match the protocol it came on, so a stranger cannot slip a ceremony message
            // through the lobby lane. The keeper still checks that the frame's signer is this peer.
            let lane_matches = Lane::of(&request) == lane;
            let accepted = lane_matches
                && swarm.behaviour_mut().gate.admission.accepts(
                    &identity,
                    lane,
                    std::time::Instant::now(),
                );
            if !accepted {
                tracing::debug!(%peer, ?lane, "dropped a frame that is not admitted on this lane");
                return;
            }
            let _ = inbound.send(Inbound {
                channel: Channel::Peer(identity),
                bytes: request,
            });
            let behaviour = swarm.behaviour_mut();
            let protocol = match lane {
                Lane::Ecdh => &mut behaviour.ecdh,
                Lane::Ceremony => &mut behaviour.ceremony,
                Lane::Sync => &mut behaviour.sync,
                Lane::Lobby => &mut behaviour.lobby,
            };
            let _ = protocol.send_response(channel, Vec::new());
        }
        request_response::Event::Message {
            peer,
            message: request_response::Message::Response { request_id, .. },
            ..
        } => {
            pending.seen(&peer);
            if let Some(reply) = pending.sends.remove(&request_id) {
                let _ = reply.send(Ok(()));
            }
        }
        request_response::Event::OutboundFailure {
            request_id, error, ..
        } => {
            if let Some(reply) = pending.sends.remove(&request_id) {
                let _ = reply.send(Err(transport_error(error)));
            }
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            tracing::debug!(%peer, %error, "inbound request failed");
        }
        request_response::Event::ResponseSent { .. } => {}
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
