//! DKG participant: round 1 broadcast, round 2 shares in envelopes, finalisation, complaint → justification, and a
//! transcript every member must confirm. Members are indexed `1..=n` in the agreed order; there is no coordinator.

use std::collections::BTreeMap;

use ome_crypto::{
    SecretScalar,
    sealing::envelope::{self, Context, TransportPublic, TransportSecret},
    threshold::{
        dkg::{self, KeyShare, Parameters, Round1Package, Round1Secret, Round2Package},
        shamir::MemberIndex,
    },
};
use ome_wire::{
    IdentityPublic, Message,
    messages::ceremony::{
        AbortReason, Ceremony, Complaint, Confirm, DkgRound1, Justification, SealedShare,
    },
    types::{Bytes64, parse_scalar, scalar},
};
use rand_core::CryptoRng;
use serde::{Deserialize, Serialize};

use super::{
    Failure, KIND_DKG_SHARE, Outbox, Participant, Status, Target, abort_message, hex_identity,
};
use crate::{Error, Result};

/// DKG parameters every participant's owner agrees to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DkgParams {
    /// Ceremony session id.
    pub session: [u8; 32],
    /// Viewing threshold `t_v`.
    pub threshold: u32,
    /// Participants; member `i` is at position `i - 1`.
    pub members: Vec<Participant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Round1,
    Round2,
    Confirm,
    Complaint {
        accused: MemberIndex,
        complainant: MemberIndex,
    },
    Done,
}

/// One participant's DKG.
pub struct Dkg {
    params: DkgParams,
    crypto: Parameters,
    me: MemberIndex,
    phase: Phase,
    deadline: u64,
    round_timeout: u64,
    secret: Option<Round1Secret>,
    round1: BTreeMap<MemberIndex, Round1Package>,
    sent: Vec<Round2Package>,
    received: BTreeMap<MemberIndex, SecretScalar>,
    confirms: BTreeMap<MemberIndex, [u8; 64]>,
    result: Option<KeyShare>,
    status: Status,
}

impl Dkg {
    /// Starts the ceremony as `me` and returns the round 1 broadcast.
    ///
    /// # Errors
    /// [`Error::Invalid`] if `me` is not a participant or the parameters are invalid.
    pub fn start(
        params: DkgParams,
        me: &IdentityPublic,
        now: u64,
        round_timeout: u64,
        rng: &mut impl CryptoRng,
    ) -> Result<(Self, Outbox)> {
        let index = index_of(&params.members, me)
            .ok_or_else(|| Error::Invalid("not a DKG participant".into()))?;
        let n = u32::try_from(params.members.len())
            .map_err(|_| Error::Invalid("too many members".into()))?;
        let crypto = Parameters {
            session: params.session,
            threshold: usize::try_from(params.threshold)
                .map_err(|_| Error::Invalid("threshold".into()))?,
            participants: (1..=n).collect(),
        };
        if crypto.threshold < 2 || n as usize > ome_wire::limits::MAX_MEMBERS {
            return Err(Error::Invalid(
                "a viewing committee has 2 <= t_v <= n <= 8".into(),
            ));
        }
        let (secret, package) = dkg::round1(&crypto, index, rng)?;
        let mut outbox = Outbox::default();
        outbox.push(Target::All, Message::DkgRound1(DkgRound1::from(&package)));
        let mut round1 = BTreeMap::new();
        round1.insert(index, package);
        Ok((
            Self {
                params,
                crypto,
                me: index,
                phase: Phase::Round1,
                deadline: now + round_timeout,
                round_timeout,
                secret: Some(secret),
                round1,
                sent: Vec::new(),
                received: BTreeMap::new(),
                confirms: BTreeMap::new(),
                result: None,
                status: Status::Running,
            },
            outbox,
        ))
    }

    /// Current status.
    #[must_use]
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// The parameters.
    #[must_use]
    pub fn params(&self) -> &DkgParams {
        &self.params
    }

    /// Takes the key share once the ceremony completed.
    pub fn take_result(&mut self) -> Option<KeyShare> {
        self.result.take()
    }

    /// The finished key share, if any (without taking it).
    #[must_use]
    pub fn result(&self) -> Option<&KeyShare> {
        self.result.as_ref()
    }

    /// Handles a message from participant `from`.
    pub fn handle(
        &mut self,
        from: &IdentityPublic,
        message: Message,
        transport: &TransportSecret,
        now: u64,
        rng: &mut impl CryptoRng,
    ) -> Outbox {
        let mut outbox = Outbox::default();
        if self.status != Status::Running {
            return outbox;
        }
        let Some(sender) = index_of(&self.params.members, from) else {
            return outbox;
        };
        match message {
            Message::DkgRound1(round1) => {
                self.on_round1(sender, &round1, transport, now, rng, &mut outbox);
            }
            Message::DkgRound2(sealed) => {
                self.on_round2(sender, &sealed, transport, now, &mut outbox);
            }
            Message::DkgConfirm(confirm) => self.on_confirm(sender, &confirm),
            Message::Complaint(complaint) if complaint.ceremony == Ceremony::Dkg => {
                self.on_complaint(sender, complaint.accused, now, &mut outbox);
            }
            Message::Justification(justification) if justification.ceremony == Ceremony::Dkg => {
                self.on_justification(sender, &justification);
            }
            Message::Abort(abort) if abort.ceremony == Ceremony::Dkg => {
                self.fail(
                    format!("aborted by member {sender}: {:?}", abort.reason),
                    &[],
                );
            }
            _ => {}
        }
        outbox
    }

    /// Advances the clock: a phase past its deadline aborts, blaming whoever has not sent what it needed.
    pub fn tick(&mut self, now: u64) -> Outbox {
        let mut outbox = Outbox::default();
        if self.status != Status::Running || now <= self.deadline {
            return outbox;
        }
        let n = self.crypto.participants.len();
        let missing = |have: &dyn Fn(MemberIndex) -> bool| -> Vec<MemberIndex> {
            (1..=member_count(n)).filter(|&i| !have(i)).collect()
        };
        let blamed = match self.phase {
            Phase::Round1 => missing(&|i| self.round1.contains_key(&i)),
            Phase::Round2 => missing(&|i| i == self.me || self.received.contains_key(&i)),
            Phase::Confirm => missing(&|i| self.confirms.contains_key(&i)),
            Phase::Complaint { accused, .. } => vec![accused],
            Phase::Done => Vec::new(),
        };
        self.fail("timed out".into(), &blamed);
        outbox.push(
            Target::All,
            abort_message(Ceremony::Dkg, AbortReason::Timeout),
        );
        outbox
    }

    fn on_round1(
        &mut self,
        sender: MemberIndex,
        wire: &DkgRound1,
        transport: &TransportSecret,
        now: u64,
        rng: &mut impl CryptoRng,
        outbox: &mut Outbox,
    ) {
        let Ok(package) = Round1Package::try_from(wire) else {
            self.fail_and_announce(
                format!("member {sender} sent a malformed round 1"),
                &[sender],
                outbox,
            );
            return;
        };
        if package.dealer != sender {
            self.fail_and_announce(
                format!("member {sender} sent round 1 for another dealer"),
                &[sender],
                outbox,
            );
            return;
        }
        if let Some(existing) = self.round1.get(&sender) {
            if *existing != package {
                self.fail_and_announce(
                    format!("member {sender} sent two different round 1 packages"),
                    &[sender],
                    outbox,
                );
            }
            return;
        }
        self.round1.insert(sender, package);
        if self.phase == Phase::Round1 && self.round1.len() == self.crypto.participants.len() {
            self.send_round2(transport, now, rng, outbox);
        }
    }

    fn send_round2(
        &mut self,
        transport: &TransportSecret,
        now: u64,
        rng: &mut impl CryptoRng,
        outbox: &mut Outbox,
    ) {
        let Some(secret) = self.secret.as_ref() else {
            return;
        };
        let packages: Vec<Round1Package> = self.round1.values().cloned().collect();
        let shares = match dkg::round2(&self.crypto, secret, &packages) {
            Ok(shares) => shares,
            Err(ome_crypto::Error::InvalidProof(dealer)) => {
                self.fail_and_announce(
                    format!("member {dealer} sent an invalid proof of knowledge"),
                    &[dealer],
                    outbox,
                );
                return;
            }
            Err(error) => {
                self.fail_and_announce(format!("round 1 rejected: {error}"), &[], outbox);
                return;
            }
        };
        for share in &shares {
            let recipient = &self.params.members[position(share.recipient)];
            let context = self.context(self.me, share.recipient);
            match envelope::seal_scalar(
                &context,
                transport,
                &TransportPublic(recipient.transport),
                &share.share,
                rng,
            ) {
                Ok(envelope) => outbox.push(
                    Target::One(recipient.identity),
                    Message::DkgRound2(SealedShare::new(self.me, share.recipient, envelope)),
                ),
                Err(error) => {
                    self.fail_and_announce(format!("cannot seal a share: {error}"), &[], outbox);
                    return;
                }
            }
        }
        self.sent = shares;
        self.phase = Phase::Round2;
        self.deadline = now + self.round_timeout;
        self.try_finalize(now, outbox);
    }

    fn on_round2(
        &mut self,
        sender: MemberIndex,
        sealed: &SealedShare,
        transport: &TransportSecret,
        now: u64,
        outbox: &mut Outbox,
    ) {
        if sealed.dealer != sender
            || sealed.recipient != self.me
            || self.received.contains_key(&sender)
        {
            return;
        }
        let context = self.context(sender, self.me);
        let sender_key = TransportPublic(self.params.members[position(sender)].transport);
        let Ok(share) = envelope::open_scalar(&context, transport, &sender_key, &sealed.envelope())
        else {
            // An unopenable share is a bad share from `sender`: complain like for a wrong value.
            self.complain(sender, now, outbox);
            return;
        };
        self.received.insert(sender, share);
        self.try_finalize(now, outbox);
    }

    fn try_finalize(&mut self, now: u64, outbox: &mut Outbox) {
        if self.phase != Phase::Round2 || self.received.len() + 1 != self.crypto.participants.len()
        {
            return;
        }
        let Some(secret) = self.secret.take() else {
            return;
        };
        let packages: Vec<Round1Package> = self.round1.values().cloned().collect();
        let received: Vec<Round2Package> = self
            .received
            .iter()
            .map(|(&dealer, share)| Round2Package {
                dealer,
                recipient: self.me,
                share: SecretScalar::new(*share.expose()),
            })
            .collect();
        match dkg::finalize(&self.crypto, secret, &packages, &received) {
            Ok(share) => {
                let transcript = *share.transcript();
                self.result = Some(share);
                self.confirms.insert(self.me, transcript);
                self.phase = Phase::Confirm;
                outbox.push(
                    Target::All,
                    Message::DkgConfirm(Confirm {
                        transcript: Bytes64::from_array(&transcript),
                    }),
                );
                self.check_confirms();
            }
            Err(ome_crypto::Error::InvalidShare(dealer)) if dealer != self.me => {
                self.complain(dealer, now, outbox);
            }
            Err(error) => {
                self.fail_and_announce(format!("finalisation failed: {error}"), &[], outbox);
            }
        }
    }

    fn complain(&mut self, accused: MemberIndex, now: u64, outbox: &mut Outbox) {
        outbox.push(
            Target::All,
            Message::Complaint(Complaint {
                ceremony: Ceremony::Dkg,
                accused,
            }),
        );
        self.phase = Phase::Complaint {
            accused,
            complainant: self.me,
        };
        self.deadline = now + self.round_timeout;
    }

    fn on_confirm(&mut self, sender: MemberIndex, confirm: &Confirm) {
        self.confirms
            .entry(sender)
            .or_insert_with(|| confirm.transcript.to_array());
        self.check_confirms();
    }

    fn check_confirms(&mut self) {
        if self.phase != Phase::Confirm || self.confirms.len() != self.crypto.participants.len() {
            return;
        }
        let Some(mine) = self.confirms.get(&self.me).copied() else {
            return;
        };
        let differing: Vec<MemberIndex> = self
            .confirms
            .iter()
            .filter(|(_, t)| **t != mine)
            .map(|(i, _)| *i)
            .collect();
        if differing.is_empty() {
            self.phase = Phase::Done;
            self.status = Status::Completed;
        } else {
            self.result = None;
            self.fail("transcripts differ (equivocation)".into(), &differing);
        }
    }

    fn on_complaint(
        &mut self,
        complainant: MemberIndex,
        accused: MemberIndex,
        now: u64,
        outbox: &mut Outbox,
    ) {
        if accused == self.me {
            if let Some(share) = self.sent.iter().find(|s| s.recipient == complainant) {
                outbox.push(
                    Target::All,
                    Message::Justification(Justification {
                        ceremony: Ceremony::Dkg,
                        complainant,
                        share: scalar(share.share.expose()),
                    }),
                );
            }
            // The share is now public: the ceremony cannot finish either way.
            self.fail(
                format!("member {complainant} disputed this member's share"),
                &[],
            );
            return;
        }
        if !matches!(self.phase, Phase::Complaint { .. }) {
            self.phase = Phase::Complaint {
                accused,
                complainant,
            };
            self.deadline = now + self.round_timeout;
        }
        self.result = None;
    }

    fn on_justification(&mut self, sender: MemberIndex, justification: &Justification) {
        let Phase::Complaint {
            accused,
            complainant,
        } = self.phase
        else {
            return;
        };
        if sender != accused || justification.complainant != complainant {
            return;
        }
        let Some(package) = self.round1.get(&accused) else {
            return;
        };
        let dealer_at_fault = match parse_scalar(&justification.share) {
            Ok(share) => dkg::complaint_is_valid(package, complainant, &SecretScalar::new(share)),
            Err(_) => true,
        };
        if dealer_at_fault {
            self.fail(
                format!("member {accused} dealt a bad share to member {complainant}"),
                &[accused],
            );
        } else {
            // Others cannot tell whether this share is the one that was sent, so nobody is blamed; the share is now
            // public and the ceremony must restart.
            self.fail(format!("member {complainant} disputed member {accused}'s share, which was revealed and is valid"), &[]);
        }
    }

    fn context(&self, sender: MemberIndex, recipient: MemberIndex) -> Context {
        Context {
            session: self.params.session,
            epoch: 0,
            kind: KIND_DKG_SHARE,
            sender,
            recipient,
        }
    }

    fn fail_and_announce(&mut self, reason: String, blamed: &[MemberIndex], outbox: &mut Outbox) {
        self.fail(reason, blamed);
        outbox.push(
            Target::All,
            abort_message(Ceremony::Dkg, AbortReason::Complaint),
        );
    }

    fn fail(&mut self, reason: String, blamed: &[MemberIndex]) {
        self.phase = Phase::Done;
        self.result = None;
        self.secret = None;
        self.sent.clear();
        self.received.clear();
        self.status = Status::Aborted(Failure {
            reason,
            blamed: blamed
                .iter()
                .filter_map(|&i| self.params.members.get(position(i)))
                .map(|p| hex_identity(&p.identity))
                .collect(),
        });
    }
}

fn index_of(members: &[Participant], identity: &IdentityPublic) -> Option<MemberIndex> {
    members
        .iter()
        .position(|m| &m.identity == identity)
        .and_then(|p| MemberIndex::try_from(p + 1).ok())
}

fn position(index: MemberIndex) -> usize {
    usize::try_from(index)
        .unwrap_or(usize::MAX)
        .saturating_sub(1)
}

fn member_count(n: usize) -> MemberIndex {
    MemberIndex::try_from(n).unwrap_or(MemberIndex::MAX)
}
