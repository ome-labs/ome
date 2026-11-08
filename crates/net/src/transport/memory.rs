use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Instant,
};

use ome_wire::IdentityPublic;
use tokio::sync::mpsc;

use super::{Channel, Inbound, Peer, PeerStatus, SendFuture, Transport, unix_now};
use crate::{Admission, Error, framing::Lane};

/// Decides whether a frame `(from, to, frame)` is delivered; used to model partitions and faults.
type Filter = Box<dyn Fn(&IdentityPublic, &IdentityPublic, &[u8]) -> bool + Send + Sync>;

struct Endpoint {
    inbound: mpsc::UnboundedSender<Inbound>,
    admission: Arc<Mutex<Admission>>,
}

/// An in-process network: every endpoint is a channel, and admission works as on libp2p. Keeper tests run many keepers
/// on it; an app can embed a keeper with it.
#[derive(Default)]
pub struct MemoryNetwork {
    endpoints: Mutex<HashMap<IdentityPublic, Endpoint>>,
    filter: Mutex<Option<Filter>>,
    /// Unix seconds of the last frame delivered, by (receiver, sender).
    seen: Mutex<HashMap<(IdentityPublic, IdentityPublic), u64>>,
}

impl MemoryNetwork {
    /// A new, empty network.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Joins the network as `identity`.
    pub fn endpoint(
        self: &Arc<Self>,
        identity: IdentityPublic,
    ) -> (MemoryTransport, mpsc::UnboundedReceiver<Inbound>) {
        let (inbound, receiver) = mpsc::unbounded_channel();
        let admission = Arc::new(Mutex::new(Admission::default()));
        lock(&self.endpoints).insert(
            identity,
            Endpoint {
                inbound,
                admission: Arc::clone(&admission),
            },
        );
        (
            MemoryTransport {
                network: Arc::clone(self),
                me: identity,
                admission,
            },
            receiver,
        )
    }

    /// Removes an endpoint, as a crashed or stopped keeper.
    pub fn leave(&self, identity: &IdentityPublic) {
        lock(&self.endpoints).remove(identity);
    }

    /// Installs a delivery filter `(from, to, frame) -> deliver?`.
    pub fn set_filter(
        &self,
        filter: impl Fn(&IdentityPublic, &IdentityPublic, &[u8]) -> bool + Send + Sync + 'static,
    ) {
        *lock(&self.filter) = Some(Box::new(filter));
    }

    /// Removes the filter.
    pub fn clear_filter(&self) {
        *lock(&self.filter) = None;
    }

    fn passes(&self, from: &IdentityPublic, to: &IdentityPublic, frame: &[u8]) -> bool {
        lock(&self.filter)
            .as_ref()
            .is_none_or(|filter| filter(from, to, frame))
    }

    /// Whether `to` is joined, admits `from`, and the filter lets frames through.
    fn reaches(&self, from: &IdentityPublic, to: &IdentityPublic) -> bool {
        lock(&self.endpoints)
            .get(to)
            .is_some_and(|e| lock(&e.admission).admits(from))
            && self.passes(from, to, &[])
    }
}

/// One keeper's endpoint on a [`MemoryNetwork`].
pub struct MemoryTransport {
    network: Arc<MemoryNetwork>,
    me: IdentityPublic,
    admission: Arc<Mutex<Admission>>,
}

impl MemoryTransport {
    fn deliver(&self, peer: &Peer, frame: Vec<u8>) -> Result<(), Error> {
        if !lock(&self.admission).admits(&peer.identity) {
            return Err(Error::NotAdmitted);
        }
        if !self.network.passes(&self.me, &peer.identity, &frame) {
            return Err(Error::Unreachable);
        }
        let endpoints = lock(&self.network.endpoints);
        let endpoint = endpoints.get(&peer.identity).ok_or(Error::Unreachable)?;
        let lane = Lane::of(&frame);
        if !lock(&endpoint.admission).accepts(&self.me, lane, Instant::now()) {
            return Err(Error::NotAdmitted);
        }
        endpoint
            .inbound
            .send(Inbound {
                channel: Channel::Peer(self.me),
                bytes: frame,
            })
            .map_err(|_| Error::Stopped)?;
        lock(&self.network.seen).insert((peer.identity, self.me), unix_now());
        Ok(())
    }
}

impl Transport for MemoryTransport {
    fn send(&self, peer: Peer, frame: Vec<u8>) -> SendFuture<'_> {
        let result = self.deliver(&peer, frame);
        Box::pin(async move { result })
    }

    fn set_admission(&self, members: Vec<Peer>, lobby_open: bool) {
        lock(&self.admission).set(members.iter().map(|p| p.identity), lobby_open);
    }

    /// Connected while both endpoints admit each other and the filter lets frames through; never relayed.
    fn peer_status(&self, peers: &[IdentityPublic]) -> Vec<PeerStatus> {
        let seen = lock(&self.network.seen);
        peers
            .iter()
            .map(|peer| PeerStatus {
                connected: self.network.reaches(&self.me, peer)
                    && self.network.reaches(peer, &self.me),
                relayed: false,
                last_seen: seen.get(&(self.me, *peer)).copied(),
                rtt_ms: None,
            })
            .collect()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
