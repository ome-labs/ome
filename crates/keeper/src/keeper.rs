//! The keeper: owns custody, the store and the transport, runs ceremonies, answers and gathers threshold ECDH, and
//! reacts to the vault configs its daemon observes on chain.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex as SyncMutex, PoisonError},
    time::Duration,
};

use ome_crypto::threshold::ecdh::AddressSecretShare;
use ome_net::{Channel, Inbound, Peer, Transport};
use ome_wire::{
    Header, IdentityPublic, Message, ReplayGuard, SignedMessage, decode, encode,
    messages::ecdh::EcdhResponse, version::PROTOCOL_VERSION,
};
use tokio::sync::{Mutex, broadcast, mpsc};

use crate::{
    Result,
    ceremony::{Participant, dkg::Dkg, reshare::Reshare},
    clock::Clock,
    committee::Committee,
    custody::{KeeperKeys, SecretStore},
    events::Event,
    limits::RateLimiter,
    random::random_bytes,
    store::{ManifestStatus, Store},
};

/// Tunables.
#[derive(Clone, Debug)]
pub struct KeeperConfig {
    /// Seconds a ceremony round may take before the ceremony aborts.
    pub round_timeout_secs: u64,
    /// How long an ECDH batch waits for other members.
    pub ecdh_timeout: Duration,
    /// ECDH budget per requesting member, in points per minute.
    pub ecdh_points_per_minute: u32,
    /// Accepted clock skew for signed messages.
    pub max_skew_secs: u64,
    /// Remembered message ids for replay protection.
    pub replay_capacity: usize,
    /// Seconds a completed reshare's pending share waits for its rotation to land before it is destroyed.
    pub pending_share_ttl_secs: u64,
}

impl Default for KeeperConfig {
    fn default() -> Self {
        Self {
            round_timeout_secs: 120,
            ecdh_timeout: Duration::from_secs(15),
            ecdh_points_per_minute: 60_000,
            max_skew_secs: 300,
            replay_capacity: 65_536,
            pending_share_ttl_secs: 7 * 24 * 3600,
        }
    }
}

/// Everything a keeper runs on.
pub struct KeeperParts {
    /// Long-lived keys.
    pub keys: KeeperKeys,
    /// Sealed secrets.
    pub secrets: SecretStore,
    /// Database.
    pub store: Store,
    /// Peer transport.
    pub transport: Box<dyn Transport>,
    /// Inbound frames of that transport.
    pub inbound: mpsc::UnboundedReceiver<Inbound>,
    /// Clock.
    pub clock: Arc<dyn Clock>,
    /// Tunables.
    pub config: KeeperConfig,
    /// Addresses this keeper is reachable at (published in manifests).
    pub addresses: Vec<String>,
}

/// A running keeper (cheap to clone).
#[derive(Clone)]
pub struct Keeper(pub(crate) Arc<Inner>);

pub(crate) struct Inner {
    pub(crate) keys: KeeperKeys,
    pub(crate) secrets: SecretStore,
    pub(crate) store: Store,
    pub(crate) transport: Box<dyn Transport>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) config: KeeperConfig,
    pub(crate) addresses: Vec<String>,
    pub(crate) state: Mutex<State>,
    pub(crate) pending: SyncMutex<HashMap<[u8; 16], Waiting>>,
    pub(crate) events: broadcast::Sender<Event>,
}

pub(crate) struct State {
    pub(crate) vaults: HashMap<[u8; 32], VaultState>,
    pub(crate) dkgs: HashMap<[u8; 32], Dkg>,
    pub(crate) reshares: HashMap<[u8; 32], Reshare>,
    /// Ceremony messages that arrived before the owner started the session (bounded).
    pub(crate) early: HashMap<[u8; 32], Vec<EarlyFrame>>,
    pub(crate) replay: ReplayGuard,
    pub(crate) limiter: RateLimiter,
    /// Dealer broadcasts this keeper already echoed, by session and dealer.
    pub(crate) echoed: HashSet<([u8; 32], IdentityPublic)>,
    /// Participants of finished ceremonies with the time they stay reachable: a member that finishes first must still
    /// deliver its last messages (and accept retries) to members that are not in any of its committees yet.
    pub(crate) lingering: HashMap<[u8; 32], (u64, Vec<Participant>)>,
}

pub(crate) struct VaultState {
    pub(crate) committee: Committee,
    pub(crate) status: ManifestStatus,
    pub(crate) share: Option<Arc<AddressSecretShare>>,
}

/// Where an ECDH batch waits for responses.
pub(crate) type Waiting = mpsc::UnboundedSender<(IdentityPublic, EcdhResponse)>;
/// A buffered frame: signer, channel, bytes.
pub(crate) type EarlyFrame = (IdentityPublic, Channel, Vec<u8>);

const EARLY_SESSIONS: usize = 16;
const EARLY_PER_SESSION: usize = 64;

impl Keeper {
    /// Starts the keeper: aborts ceremonies interrupted by a restart, loads every vault and its share, and spawns the
    /// inbound and timer loops.
    ///
    /// # Errors
    /// Storage or custody errors while loading.
    pub fn start(parts: KeeperParts) -> Result<Self> {
        let KeeperParts {
            keys,
            secrets,
            store,
            transport,
            mut inbound,
            clock,
            config,
            addresses,
        } = parts;
        let now = clock.now();
        for session in store.running_ceremonies()? {
            // Ceremony secrets were only in memory: the ceremony cannot resume.
            store.update_ceremony(
                &session,
                "aborted",
                Some(&crate::ceremony::Failure {
                    reason: "keeper restarted during the ceremony".into(),
                    blamed: Vec::new(),
                }),
                now,
            )?;
        }
        let state = State {
            vaults: HashMap::new(),
            dkgs: HashMap::new(),
            reshares: HashMap::new(),
            early: HashMap::new(),
            replay: ReplayGuard::new(config.max_skew_secs, config.replay_capacity),
            limiter: RateLimiter::new(config.ecdh_points_per_minute),
            echoed: HashSet::new(),
            lingering: HashMap::new(),
        };
        let (events, _) = broadcast::channel(1024);
        let keeper = Self(Arc::new(Inner {
            keys,
            secrets,
            store,
            transport,
            clock,
            config,
            addresses,
            state: Mutex::new(state),
            pending: SyncMutex::new(HashMap::new()),
            events,
        }));
        keeper.load_vaults()?;

        let receiver = keeper.clone();
        tokio::spawn(async move {
            while let Some(frame) = inbound.recv().await {
                let keeper = receiver.clone();
                tokio::spawn(async move { keeper.on_inbound(frame).await });
            }
        });
        let ticker = Arc::downgrade(&keeper.0);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let Some(inner) = ticker.upgrade() else { break };
                Keeper(inner).tick().await;
            }
        });
        Ok(keeper)
    }

    /// This keeper's signing identity.
    #[must_use]
    pub fn identity(&self) -> IdentityPublic {
        self.0.keys.identity.public()
    }

    /// This keeper's transport public key.
    #[must_use]
    pub fn transport_key(&self) -> [u8; 32] {
        self.0.keys.transport_public.0
    }

    /// Addresses this keeper advertises.
    #[must_use]
    pub fn addresses(&self) -> &[String] {
        &self.0.addresses
    }

    /// Subscribes to events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.0.events.subscribe()
    }

    pub(crate) fn emit(&self, event: Event) {
        let _ = self.0.events.send(event);
    }

    pub(crate) fn now(&self) -> u64 {
        self.0.clock.now()
    }

    /// Signs `body` under `scope` / `epoch` with a fresh message id.
    pub(crate) fn sign(
        &self,
        scope: [u8; 32],
        epoch: u32,
        body: Message,
    ) -> Result<(Vec<u8>, [u8; 16])> {
        let id = random_bytes();
        let header = Header {
            scope,
            epoch,
            id,
            created_at: self.now(),
        };
        let message = SignedMessage::sign(&self.0.keys.identity, PROTOCOL_VERSION, header, body)?;
        Ok((encode(&message)?, id))
    }

    /// Sends a frame, logging failures (protocols time out and report missing members). Returns whether the transport
    /// delivered it.
    pub(crate) async fn send(&self, peer: Peer, frame: Vec<u8>) -> bool {
        let identity = hex::encode(peer.identity.0);
        match self.0.transport.send(peer, frame).await {
            Ok(()) => true,
            Err(error) => {
                tracing::debug!(peer = %identity, %error, "send failed");
                false
            }
        }
    }

    /// Sends with exponential backoff and jitter (from 200 ms, at most 10 s apart) until delivered or `give_up` passed.
    pub(crate) async fn send_with_retry(
        &self,
        peer: Peer,
        frame: Vec<u8>,
        give_up: Duration,
    ) -> bool {
        let deadline = tokio::time::Instant::now() + give_up;
        let mut backoff = Duration::from_millis(200);
        loop {
            if self.send(peer.clone(), frame.clone()).await {
                return true;
            }
            let jitter = Duration::from_millis(u64::from(random_bytes::<1>()[0]));
            let wait = backoff + jitter;
            if tokio::time::Instant::now() + wait >= deadline {
                return false;
            }
            tokio::time::sleep(wait).await;
            backoff = (backoff * 2).min(Duration::from_secs(10));
        }
    }

    /// Refreshes transport admission from every committee and ceremony.
    pub(crate) fn refresh_allowlist(&self, state: &State) {
        let me = self.identity();
        let mut peers = Vec::new();
        for vault in state
            .vaults
            .values()
            .filter(|v| v.status == ManifestStatus::Active)
        {
            peers.extend(vault.committee.peers_except(&me));
        }
        for dkg in state.dkgs.values() {
            peers.extend(
                dkg.params()
                    .members
                    .iter()
                    .filter(|p| p.identity != me)
                    .map(Peer::from),
            );
        }
        for (_, participants) in state.lingering.values() {
            peers.extend(
                participants
                    .iter()
                    .filter(|p| p.identity != me)
                    .map(Peer::from),
            );
        }
        for reshare in state.reshares.values() {
            let params = reshare.params();
            peers.extend(
                params
                    .old_members
                    .iter()
                    .chain(&params.new_members)
                    .filter(|p| p.identity != me)
                    .map(Peer::from),
            );
        }
        peers.sort_by_key(|p| p.transport);
        peers.dedup_by_key(|p| p.transport);
        self.0.transport.set_admission(peers, false);
    }

    /// The transport key a known participant must use, if `identity` is known.
    fn expected_transport(state: &State, identity: &IdentityPublic) -> Option<[u8; 32]> {
        state
            .vaults
            .values()
            .find_map(|v| v.committee.member(identity).map(|m| m.transport))
            .or_else(|| {
                state.dkgs.values().find_map(|d| {
                    d.params()
                        .members
                        .iter()
                        .find(|p| &p.identity == identity)
                        .map(|p| p.transport)
                })
            })
            .or_else(|| {
                state.reshares.values().find_map(|r| {
                    let params = r.params();
                    params
                        .old_members
                        .iter()
                        .chain(&params.new_members)
                        .find(|p| &p.identity == identity)
                        .map(|p| p.transport)
                })
            })
    }

    pub(crate) async fn on_inbound(&self, frame: Inbound) {
        let Ok(message) = decode(&frame.bytes) else {
            tracing::debug!("dropped malformed frame");
            return;
        };
        if message.verify().is_err() {
            tracing::debug!("dropped frame with an invalid signature");
            return;
        }
        let signer = message.signer;
        let scope = message.header.scope;
        let mut state = self.0.state.lock().await;
        let bound = match (Self::expected_transport(&state, &signer), frame.channel) {
            (Some(expected), Channel::Key(key)) => Some(expected == key),
            (Some(_), Channel::Peer(peer)) => Some(peer == signer),
            (None, _) => None,
        };
        match bound {
            Some(true) => {}
            Some(false) => {
                tracing::warn!("dropped frame: identity does not match the channel");
                return;
            }
            None if is_ceremony_message(&message.body) => {
                // Possibly a ceremony this keeper's owner has not started yet: keep a bounded backlog.
                if state.early.len() < EARLY_SESSIONS || state.early.contains_key(&scope) {
                    let backlog = state.early.entry(scope).or_default();
                    if backlog.len() < EARLY_PER_SESSION {
                        backlog.push((signer, frame.channel, frame.bytes.clone()));
                    }
                }
                return;
            }
            None => return,
        }
        let expected_epoch = Self::expected_epoch(&state, &message);
        let now = self.now();
        if let Err(error) = state
            .replay
            .check(&signer, &message.header, expected_epoch, now)
        {
            tracing::debug!(%error, "dropped frame");
            return;
        }
        drop(state);
        if matches!(
            message.body,
            Message::DkgRound1(_) | Message::ReshareDeal(_)
        ) {
            self.echo(scope, signer, &frame.bytes).await;
        }
        match message.body {
            Message::EcdhRequest(request) => {
                self.on_ecdh_request(signer, message.header, request).await;
            }
            Message::EcdhResponse(response) => self.on_ecdh_response(signer, response),
            Message::Echo(echo) => self.on_echo(scope, &echo).await,
            body if is_ceremony_message(&body) => {
                self.on_ceremony_message(signer, scope, body).await;
            }
            _ => {}
        }
    }

    fn expected_epoch(state: &State, message: &SignedMessage) -> u32 {
        let scope = message.header.scope;
        if let Some(reshare) = state.reshares.get(&scope) {
            return reshare.params().old_epoch;
        }
        if state.dkgs.contains_key(&scope) {
            return 0;
        }
        state
            .vaults
            .get(&scope)
            .map_or(message.header.epoch, |v| v.committee.epoch())
    }

    /// Delivers an ECDH response to the batch waiting for it.
    fn on_ecdh_response(&self, signer: IdentityPublic, response: EcdhResponse) {
        let waiting = self
            .0
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&response.request)
            .cloned();
        if let Some(tx) = waiting {
            let _ = tx.send((signer, response));
        }
    }
}

fn is_ceremony_message(body: &Message) -> bool {
    matches!(
        body,
        Message::DkgRound1(_)
            | Message::DkgRound2(_)
            | Message::DkgConfirm(_)
            | Message::ReshareDeal(_)
            | Message::ReshareSubShare(_)
            | Message::ReshareConfirm(_)
            | Message::Complaint(_)
            | Message::Justification(_)
            | Message::Abort(_)
            | Message::Echo(_)
    )
}
