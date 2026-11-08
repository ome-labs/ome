//! Who may connect, and what a stranger may say.
//!
//! Members of the committees and ceremonies a keeper serves, and its relays, may always connect. While the keeper
//! hosts an open lobby anyone may connect so co-founders can join, but a stranger's frames are accepted only on the
//! lobby lane, at most [`LOBBY_REQUESTS_PER_MINUTE`] a minute. Every other connection is refused before any protocol
//! runs. Both transports enforce the same [`Admission`].

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use ome_wire::IdentityPublic;

use crate::framing::Lane;

/// Lobby requests a stranger may send per minute.
pub const LOBBY_REQUESTS_PER_MINUTE: u32 = 10;
/// Most strangers whose lobby rate is tracked at once, so the table cannot be flooded.
const MAX_TRACKED_STRANGERS: usize = 4096;
const MINUTE: Duration = Duration::from_secs(60);

/// Admission state of one node.
#[derive(Debug, Default)]
pub struct Admission {
    members: HashSet<IdentityPublic>,
    relays: HashSet<IdentityPublic>,
    lobby_open: bool,
    lobby_rate: HashMap<IdentityPublic, (Instant, u32)>,
}

impl Admission {
    /// Replaces the members and the lobby flag. Closing the lobby forgets every stranger's rate.
    pub fn set(&mut self, members: impl IntoIterator<Item = IdentityPublic>, lobby_open: bool) {
        self.members = members.into_iter().collect();
        self.lobby_open = lobby_open;
        if !lobby_open {
            self.lobby_rate.clear();
        }
    }

    /// Adds a relay, which may always connect.
    pub fn add_relay(&mut self, relay: IdentityPublic) {
        self.relays.insert(relay);
    }

    /// Whether `peer` is a member or relay, as opposed to a stranger admitted by an open lobby.
    #[must_use]
    pub fn is_known(&self, peer: &IdentityPublic) -> bool {
        self.members.contains(peer) || self.relays.contains(peer)
    }

    /// Whether `peer` is a member this node may send frames to.
    #[must_use]
    pub fn is_member(&self, peer: &IdentityPublic) -> bool {
        self.members.contains(peer)
    }

    /// Whether a connection with `peer` may exist.
    #[must_use]
    pub fn admits(&self, peer: &IdentityPublic) -> bool {
        self.lobby_open || self.is_known(peer)
    }

    /// Whether a frame from `peer` on `lane` is delivered at time `now`. Members are delivered on every lane. A stranger
    /// only on the lobby lane, while the lobby is open, within its rate; each call counts as a request.
    pub fn accepts(&mut self, peer: &IdentityPublic, lane: Lane, now: Instant) -> bool {
        if self.members.contains(peer) {
            return true;
        }
        lane == Lane::Lobby && self.lobby_open && self.lobby_request_allowed(*peer, now)
    }

    fn lobby_request_allowed(&mut self, peer: IdentityPublic, now: Instant) -> bool {
        if self.lobby_rate.len() >= MAX_TRACKED_STRANGERS && !self.lobby_rate.contains_key(&peer) {
            self.lobby_rate
                .retain(|_, (start, _)| now.duration_since(*start) < MINUTE);
            if self.lobby_rate.len() >= MAX_TRACKED_STRANGERS {
                return false;
            }
        }
        let (start, count) = self.lobby_rate.entry(peer).or_insert((now, 0));
        if now.duration_since(*start) >= MINUTE {
            *start = now;
            *count = 0;
        }
        *count += 1;
        *count <= LOBBY_REQUESTS_PER_MINUTE
    }
}
