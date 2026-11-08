//! Transport-agnostic ceremony state machines (DKG and reshare). Inputs are signed messages from known participants
//! and clock ticks; outputs are messages to send and a status. Ceremony secrets (polynomials, received shares) live
//! only in memory: a keeper that restarts mid-ceremony aborts it and members start a new session.

pub mod dkg;
pub mod reshare;

use ome_wire::{
    IdentityPublic, Message,
    messages::ceremony::{Abort, AbortReason, Ceremony},
};
use serde::{Deserialize, Serialize};

use ome_net::Peer;

/// Envelope kinds (bound into the HPKE context).
pub(crate) const KIND_DKG_SHARE: u16 = 1;
/// Reshare sub-share envelope kind.
pub(crate) const KIND_RESHARE_SUB_SHARE: u16 = 2;

/// Who a message goes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Every other participant.
    All,
    /// One participant.
    One(IdentityPublic),
}

/// Messages a state machine wants sent.
#[derive(Debug, Default)]
pub struct Outbox(pub Vec<(Target, Message)>);

impl Outbox {
    fn push(&mut self, target: Target, message: Message) {
        self.0.push((target, message));
    }
}

/// Why a ceremony stopped without a result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// Reason.
    pub reason: String,
    /// Participants held responsible (identities, hex), if any.
    pub blamed: Vec<String>,
}

/// Ceremony status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Still running.
    Running,
    /// Finished successfully.
    Completed,
    /// Stopped; see [`Failure`].
    Aborted(Failure),
}

impl Status {
    /// Short status label.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Aborted(_) => "aborted",
        }
    }
}

/// A ceremony participant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    /// Signing identity.
    pub identity: IdentityPublic,
    /// X25519 transport key.
    pub transport: [u8; 32],
    /// Network addresses.
    pub addresses: Vec<String>,
}

impl From<&Participant> for Peer {
    fn from(p: &Participant) -> Self {
        Self {
            identity: p.identity,
            transport: p.transport,
            addresses: p.addresses.clone(),
        }
    }
}

pub(crate) fn hex_identity(identity: &IdentityPublic) -> String {
    hex::encode(identity.0)
}

pub(crate) fn abort_message(ceremony: Ceremony, reason: AbortReason) -> Message {
    Message::Abort(Abort { ceremony, reason })
}
