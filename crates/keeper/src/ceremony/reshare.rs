//! Reshare participant: `t_v` current members deal sub-shares of their weighted shares to the new committee; new
//! members check every dealer against the current committee's public verification shares, combine, and confirm one
//! transcript. The result stays pending until the vault's on-chain config names the new committee.

use std::collections::BTreeMap;

use ome_crypto::{
    Point, SecretScalar,
    sealing::envelope::{self, Context, TransportPublic, TransportSecret},
    threshold::{
        dkg::KeyShare,
        reshare::{self, DealerPackage, Parameters, SubShare},
        shamir::{Commitments, MemberIndex},
    },
};
use ome_wire::{
    IdentityPublic, Message,
    messages::ceremony::{
        AbortReason, Ceremony, Complaint, Confirm, Justification, ReshareDeal, SealedShare,
    },
    types::{Bytes64, WirePoint, parse_points, parse_scalar, scalar},
};
use rand_core::CryptoRng;
use serde::{Deserialize, Serialize};

use super::{
    Failure, KIND_RESHARE_SUB_SHARE, Outbox, Participant, Status, Target, abort_message,
    hex_identity,
};
use crate::{Error, Result};

/// Reshare parameters every participant's owner agrees to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReshareParams {
    /// Ceremony session id.
    pub session: [u8; 32],
    /// Vault address.
    pub vault: [u8; 32],
    /// Current committee epoch.
    pub old_epoch: u32,
    /// Current viewing threshold.
    pub old_threshold: u32,
    /// Current committee, member `i` at position `i - 1`.
    pub old_members: Vec<Participant>,
    /// Current verification shares, in member order (from the manifest).
    pub old_verification_shares: Vec<WirePoint>,
    /// Exactly `old_threshold` current members who deal.
    pub dealers: Vec<MemberIndex>,
    /// New viewing threshold.
    pub new_threshold: u32,
    /// New committee, member `j` at position `j - 1`.
    pub new_members: Vec<Participant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Collect,
    Confirm,
    Complaint {
        accused: MemberIndex,
        complainant: MemberIndex,
    },
    Done,
}

/// One participant's reshare.
pub struct Reshare {
    params: ReshareParams,
    crypto: Parameters,
    old_commitments: Commitments,
    me_dealer: Option<MemberIndex>,
    me_new: Option<MemberIndex>,
    phase: Phase,
    deadline: u64,
    round_timeout: u64,
    deals: BTreeMap<MemberIndex, DealerPackage>,
    sub_shares: BTreeMap<MemberIndex, SecretScalar>,
    sent: Vec<SubShare>,
    confirms: BTreeMap<MemberIndex, [u8; 64]>,
    result: Option<KeyShare>,
    status: Status,
}

impl Reshare {
    /// Starts the reshare as `me`; dealers pass their current share and broadcast their deal.
    ///
    /// # Errors
    /// [`Error::Invalid`] for inconsistent parameters or a dealer without its share.
    pub fn start(
        params: ReshareParams,
        me: &IdentityPublic,
        current: Option<&KeyShare>,
        now: u64,
        round_timeout: u64,
        rng: &mut impl CryptoRng,
    ) -> Result<(Self, Outbox)> {
        let old_index = index_of(&params.old_members, me);
        let me_dealer = old_index.filter(|i| params.dealers.contains(i));
        let me_new = index_of(&params.new_members, me);
        if me_dealer.is_none() && me_new.is_none() {
            return Err(Error::Invalid("neither a dealer nor a new member".into()));
        }
        let n_new = u32::try_from(params.new_members.len())
            .map_err(|_| Error::Invalid("too many members".into()))?;
        let crypto = Parameters {
            session: params.session,
            dealers: params.dealers.clone(),
            old_threshold: usize::try_from(params.old_threshold)
                .map_err(|_| Error::Invalid("threshold".into()))?,
            new_participants: (1..=n_new).collect(),
            new_threshold: usize::try_from(params.new_threshold)
                .map_err(|_| Error::Invalid("threshold".into()))?,
        };
        crypto.validate()?;
        if crypto.new_threshold < 2 || crypto.new_participants.len() > ome_wire::limits::MAX_MEMBERS
        {
            return Err(Error::Invalid(
                "a viewing committee has 2 <= t_v <= n <= 8".into(),
            ));
        }
        let n_old = params.old_members.len();
        if params.old_verification_shares.len() != n_old
            || params.dealers.iter().any(|&d| position(d) >= n_old)
        {
            return Err(Error::Invalid(
                "dealers must be members of the current committee".into(),
            ));
        }
        let evaluations: Vec<_> = (1..)
            .zip(parse_points(&params.old_verification_shares)?)
            .collect();
        let old_commitments = Commitments::interpolate(crypto.old_threshold, &evaluations)?;

        let mut this = Self {
            params,
            crypto,
            old_commitments,
            me_dealer,
            me_new,
            phase: Phase::Collect,
            deadline: now + round_timeout,
            round_timeout,
            deals: BTreeMap::new(),
            sub_shares: BTreeMap::new(),
            sent: Vec::new(),
            confirms: BTreeMap::new(),
            result: None,
            status: Status::Running,
        };
        let mut outbox = Outbox::default();
        if let Some(dealer) = me_dealer {
            let current =
                current.ok_or_else(|| Error::Invalid("dealer has no current share".into()))?;
            if current.index() != dealer {
                return Err(Error::Invalid(
                    "current share belongs to another member".into(),
                ));
            }
            this.deal(current, rng, &mut outbox)?;
        }
        if this.me_new.is_none() {
            this.phase = Phase::Confirm;
        }
        this.try_receive(now, &mut outbox);
        Ok((this, outbox))
    }

    /// Current status.
    #[must_use]
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// The parameters.
    #[must_use]
    pub fn params(&self) -> &ReshareParams {
        &self.params
    }

    /// This keeper's index in the new committee, if it is a member.
    #[must_use]
    pub fn new_index(&self) -> Option<MemberIndex> {
        self.me_new
    }

    /// The new key share (new members, once completed).
    #[must_use]
    pub fn result(&self) -> Option<&KeyShare> {
        self.result.as_ref()
    }

    /// Takes the new key share.
    pub fn take_result(&mut self) -> Option<KeyShare> {
        self.result.take()
    }

    /// The new committee's verification shares, from the dealers' commitments: every participant can compute them once
    /// it holds every deal, including a dealer that leaves the committee and so receives no share.
    ///
    /// # Errors
    /// [`Error::Invalid`] before every deal arrived, or crypto errors.
    pub fn new_verification_shares(&self) -> Result<Vec<Point>> {
        if self.deals.len() != self.params.dealers.len() {
            return Err(Error::Invalid("not every dealer has dealt".into()));
        }
        let sum = Commitments::sum(self.deals.values().map(|package| &package.commitments))?;
        Ok(self
            .crypto
            .new_participants
            .iter()
            .map(|&j| sum.evaluate(j))
            .collect::<Result<Vec<_>, _>>()?)
    }

    fn deal(
        &mut self,
        current: &KeyShare,
        rng: &mut impl CryptoRng,
        outbox: &mut Outbox,
    ) -> Result<()> {
        let dealer = current.index();
        let (package, sub_shares) = reshare::deal(&self.crypto, current, rng)?;
        outbox.push(
            Target::All,
            Message::ReshareDeal(ReshareDeal::from(&package)),
        );
        for sub_share in &sub_shares {
            if Some(sub_share.recipient) == self.me_new {
                self.sub_shares
                    .insert(dealer, SecretScalar::new(*sub_share.share.expose()));
            }
        }
        self.deals.insert(dealer, package);
        self.sent = sub_shares;
        Ok(())
    }

    /// Seals this dealer's sub-shares (needs the transport key, so separate from [`Reshare::start`]).
    ///
    /// # Errors
    /// Envelope errors.
    pub fn seal_sub_shares(
        &self,
        transport: &TransportSecret,
        rng: &mut impl CryptoRng,
    ) -> Result<Outbox> {
        let mut outbox = Outbox::default();
        let Some(dealer) = self.me_dealer else {
            return Ok(outbox);
        };
        for sub_share in self
            .sent
            .iter()
            .filter(|s| Some(s.recipient) != self.me_new)
        {
            let recipient = &self.params.new_members[position(sub_share.recipient)];
            let envelope = envelope::seal_scalar(
                &self.context(dealer, sub_share.recipient),
                transport,
                &TransportPublic(recipient.transport),
                &sub_share.share,
                rng,
            )?;
            outbox.push(
                Target::One(recipient.identity),
                Message::ReshareSubShare(SealedShare::new(dealer, sub_share.recipient, envelope)),
            );
        }
        Ok(outbox)
    }

    /// Handles a message from `from`.
    pub fn handle(
        &mut self,
        from: &IdentityPublic,
        message: Message,
        transport: &TransportSecret,
        now: u64,
    ) -> Outbox {
        let mut outbox = Outbox::default();
        if self.status != Status::Running {
            return outbox;
        }
        let dealer =
            index_of(&self.params.old_members, from).filter(|i| self.params.dealers.contains(i));
        let new_member = index_of(&self.params.new_members, from);
        match message {
            Message::ReshareDeal(deal) => {
                if let Some(dealer) = dealer {
                    self.on_deal(dealer, &deal, now, &mut outbox);
                }
            }
            Message::ReshareSubShare(sealed) => {
                if let Some(dealer) = dealer {
                    self.on_sub_share(dealer, &sealed, transport, now, &mut outbox);
                }
            }
            Message::ReshareConfirm(confirm) => {
                if let Some(member) = new_member {
                    self.confirms
                        .entry(member)
                        .or_insert_with(|| confirm.transcript.to_array());
                    self.check_confirms();
                }
            }
            Message::Complaint(complaint) if complaint.ceremony == Ceremony::Reshare => {
                if let Some(complainant) = new_member {
                    self.on_complaint(complainant, complaint.accused, now, &mut outbox);
                }
            }
            Message::Justification(justification)
                if justification.ceremony == Ceremony::Reshare =>
            {
                if let Some(dealer) = dealer {
                    self.on_justification(dealer, &justification);
                }
            }
            Message::Abort(abort)
                if abort.ceremony == Ceremony::Reshare
                    && (dealer.is_some() || new_member.is_some()) =>
            {
                self.fail(
                    format!("aborted by {}: {:?}", hex_identity(from), abort.reason),
                    Vec::new(),
                );
            }
            _ => {}
        }
        outbox
    }

    /// Advances the clock; a phase past its deadline aborts.
    pub fn tick(&mut self, now: u64) -> Outbox {
        let mut outbox = Outbox::default();
        if self.status != Status::Running || now <= self.deadline {
            return outbox;
        }
        let blamed = match self.phase {
            Phase::Collect => self
                .params
                .dealers
                .iter()
                .filter(|d| !self.deals.contains_key(d) || !self.sub_shares.contains_key(d))
                .filter_map(|&d| self.params.old_members.get(position(d)))
                .map(|p| hex_identity(&p.identity))
                .collect(),
            Phase::Confirm => self
                .params
                .new_members
                .iter()
                .enumerate()
                .filter(|(p, _)| {
                    !self
                        .confirms
                        .contains_key(&(u32::try_from(*p).unwrap_or(u32::MAX) + 1))
                })
                .map(|(_, m)| hex_identity(&m.identity))
                .collect(),
            Phase::Complaint { accused, .. } => self
                .params
                .old_members
                .get(position(accused))
                .map(|p| vec![hex_identity(&p.identity)])
                .unwrap_or_default(),
            Phase::Done => Vec::new(),
        };
        self.fail_identities("timed out".into(), blamed);
        outbox.push(
            Target::All,
            abort_message(Ceremony::Reshare, AbortReason::Timeout),
        );
        outbox
    }

    fn on_deal(&mut self, dealer: MemberIndex, deal: &ReshareDeal, now: u64, outbox: &mut Outbox) {
        let Ok(package) = DealerPackage::try_from(deal) else {
            self.fail_and_announce(
                format!("dealer {dealer} sent a malformed deal"),
                &[dealer],
                outbox,
            );
            return;
        };
        if package.dealer != dealer
            || reshare::verify_dealer(&self.crypto, &self.old_commitments, &package).is_err()
        {
            self.fail_and_announce(
                format!("dealer {dealer} dealt something other than its share"),
                &[dealer],
                outbox,
            );
            return;
        }
        if let Some(existing) = self.deals.get(&dealer) {
            if *existing != package {
                self.fail_and_announce(
                    format!("dealer {dealer} sent two different deals"),
                    &[dealer],
                    outbox,
                );
            }
            return;
        }
        self.deals.insert(dealer, package);
        self.try_receive(now, outbox);
    }

    fn on_sub_share(
        &mut self,
        dealer: MemberIndex,
        sealed: &SealedShare,
        transport: &TransportSecret,
        now: u64,
        outbox: &mut Outbox,
    ) {
        let Some(me) = self.me_new else { return };
        if sealed.dealer != dealer
            || sealed.recipient != me
            || self.sub_shares.contains_key(&dealer)
        {
            return;
        }
        let sender = TransportPublic(self.params.old_members[position(dealer)].transport);
        match envelope::open_scalar(
            &self.context(dealer, me),
            transport,
            &sender,
            &sealed.envelope(),
        ) {
            Ok(share) => {
                self.sub_shares.insert(dealer, share);
                self.try_receive(now, outbox);
            }
            Err(_) => self.complain(dealer, now, outbox),
        }
    }

    fn try_receive(&mut self, now: u64, outbox: &mut Outbox) {
        let Some(me) = self.me_new else { return };
        let dealers = self.params.dealers.len();
        if self.phase != Phase::Collect
            || self.deals.len() != dealers
            || self.sub_shares.len() != dealers
        {
            return;
        }
        let packages: Vec<DealerPackage> = self.deals.values().cloned().collect();
        let sub_shares: Vec<SubShare> = self
            .sub_shares
            .iter()
            .map(|(&dealer, share)| SubShare {
                dealer,
                recipient: me,
                share: SecretScalar::new(*share.expose()),
            })
            .collect();
        match reshare::receive(
            &self.crypto,
            &self.old_commitments,
            me,
            &packages,
            &sub_shares,
        ) {
            Ok(share) => {
                let transcript = *share.transcript();
                self.result = Some(share);
                self.confirms.insert(me, transcript);
                self.phase = Phase::Confirm;
                self.deadline = now + self.round_timeout;
                outbox.push(
                    Target::All,
                    Message::ReshareConfirm(Confirm {
                        transcript: Bytes64::from_array(&transcript),
                    }),
                );
                self.check_confirms();
            }
            Err(ome_crypto::Error::InvalidShare(dealer)) => self.complain(dealer, now, outbox),
            Err(error) => {
                self.fail_and_announce(format!("cannot combine sub-shares: {error}"), &[], outbox);
            }
        }
    }

    fn check_confirms(&mut self) {
        if self.phase != Phase::Confirm || self.confirms.len() != self.params.new_members.len() {
            return;
        }
        let reference = self
            .me_new
            .and_then(|me| self.confirms.get(&me))
            .or_else(|| self.confirms.values().next())
            .copied();
        let Some(reference) = reference else { return };
        let differing: Vec<MemberIndex> = self
            .confirms
            .iter()
            .filter(|(_, t)| **t != reference)
            .map(|(i, _)| *i)
            .collect();
        if differing.is_empty() {
            self.phase = Phase::Done;
            self.status = Status::Completed;
        } else {
            self.result = None;
            let blamed = differing
                .iter()
                .filter_map(|&j| self.params.new_members.get(position(j)))
                .map(|p| hex_identity(&p.identity))
                .collect();
            self.fail_identities("transcripts differ (equivocation)".into(), blamed);
        }
    }

    fn complain(&mut self, accused: MemberIndex, now: u64, outbox: &mut Outbox) {
        let Some(me) = self.me_new else { return };
        outbox.push(
            Target::All,
            Message::Complaint(Complaint {
                ceremony: Ceremony::Reshare,
                accused,
            }),
        );
        self.phase = Phase::Complaint {
            accused,
            complainant: me,
        };
        self.deadline = now + self.round_timeout;
        self.result = None;
    }

    fn on_complaint(
        &mut self,
        complainant: MemberIndex,
        accused: MemberIndex,
        now: u64,
        outbox: &mut Outbox,
    ) {
        if Some(accused) == self.me_dealer {
            if let Some(sub_share) = self.sent.iter().find(|s| s.recipient == complainant) {
                outbox.push(
                    Target::All,
                    Message::Justification(Justification {
                        ceremony: Ceremony::Reshare,
                        complainant,
                        share: scalar(sub_share.share.expose()),
                    }),
                );
            }
            // The sub-share is now public: the reshare cannot finish either way.
            self.fail_identities(
                format!("new member {complainant} disputed this dealer's sub-share"),
                Vec::new(),
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

    fn on_justification(&mut self, dealer: MemberIndex, justification: &Justification) {
        let Phase::Complaint {
            accused,
            complainant,
        } = self.phase
        else {
            return;
        };
        if dealer != accused || justification.complainant != complainant {
            return;
        }
        let Some(package) = self.deals.get(&accused) else {
            return;
        };
        let valid = parse_scalar(&justification.share).is_ok_and(|share| {
            package
                .commitments
                .verify_share(complainant, &SecretScalar::new(share))
        });
        if valid {
            // Unattributable (nobody else saw what was sent); the sub-share is now public, so restart.
            self.fail_identities(
                format!("new member {complainant} disputed dealer {accused}'s sub-share, which was revealed and is valid"),
                Vec::new(),
            );
        } else {
            let blamed = self
                .params
                .old_members
                .get(position(accused))
                .map(|p| hex_identity(&p.identity));
            self.fail_identities(
                format!("dealer {accused} dealt a bad sub-share"),
                blamed.into_iter().collect(),
            );
        }
    }

    fn context(&self, dealer: MemberIndex, recipient: MemberIndex) -> Context {
        Context {
            session: self.params.session,
            epoch: self.params.old_epoch,
            kind: KIND_RESHARE_SUB_SHARE,
            sender: dealer,
            recipient,
        }
    }

    fn fail_and_announce(&mut self, reason: String, dealers: &[MemberIndex], outbox: &mut Outbox) {
        let blamed = dealers
            .iter()
            .filter_map(|&d| self.params.old_members.get(position(d)))
            .map(|p| hex_identity(&p.identity))
            .collect();
        self.fail_identities(reason, blamed);
        outbox.push(
            Target::All,
            abort_message(Ceremony::Reshare, AbortReason::Complaint),
        );
    }

    fn fail(&mut self, reason: String, blamed: Vec<String>) {
        self.fail_identities(reason, blamed);
    }

    fn fail_identities(&mut self, reason: String, blamed: Vec<String>) {
        self.phase = Phase::Done;
        self.result = None;
        self.sent.clear();
        self.sub_shares.clear();
        self.status = Status::Aborted(Failure { reason, blamed });
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
