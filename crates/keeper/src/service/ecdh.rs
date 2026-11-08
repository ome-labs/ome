use std::{
    collections::BTreeSet,
    sync::{Arc, PoisonError},
};

use ome_crypto::{
    Point,
    threshold::ecdh::{self, AddressSecretShare, EcdhContext, Partial},
    threshold::shamir::MemberIndex,
};
use ome_wire::{
    Header, IdentityPublic, Message,
    messages::ecdh::{EcdhPurpose, EcdhRequest, EcdhResponse},
    types::{WirePoint, parse_points},
};
use tokio::{sync::mpsc, time::Instant};

use crate::committee::Committee;
use crate::keeper::Keeper;
use crate::{
    Error, Result,
    events::Event,
    random::os_rng,
    store::{AuditEntry, ManifestStatus},
};

/// One member's partials for a batch, in input order.
#[derive(Clone, Debug)]
pub struct MemberPartials {
    /// Member index.
    pub member: MemberIndex,
    /// Partials.
    pub partials: Vec<Partial>,
}

/// What the responder needs, copied out of the state lock.
struct Serving {
    share: Arc<AddressSecretShare>,
    context: EcdhContext,
}

fn purpose_label(purpose: EcdhPurpose) -> &'static str {
    match purpose {
        EcdhPurpose::NoteDecryption => "decryption",
        EcdhPurpose::Handshake => "handshake",
        EcdhPurpose::RecipientTag => "recipient-tag",
        EcdhPurpose::SenderTag => "sender-tag",
    }
}

impl Keeper {
    /// The latest `limit` ECDH audit entries of `vault`, newest first: every request this keeper served or refused.
    ///
    /// # Errors
    /// Storage errors.
    pub fn audit_log(&self, vault: [u8; 32], limit: u32) -> Result<Vec<AuditEntry>> {
        self.0.store.audit_log(&vault, limit)
    }

    /// Checks a request against the vault's committee and budget, and returns what is needed to serve it.
    async fn admit(
        &self,
        vault: &[u8; 32],
        epoch: u32,
        requester: Option<&IdentityPublic>,
        points: usize,
    ) -> Result<Serving> {
        let now = self.now();
        let mut state = self.0.state.lock().await;
        let state = &mut *state;
        let vault_state = state.vaults.get(vault).ok_or(Error::UnknownVault)?;
        if vault_state.status == ManifestStatus::Removed {
            return Err(Error::Removed);
        }
        let share = vault_state.share.clone().ok_or(Error::UnknownVault)?;
        if vault_state.status != ManifestStatus::Active || epoch != vault_state.committee.epoch() {
            return Err(Error::WrongEpoch {
                expected: vault_state.committee.epoch(),
                actual: epoch,
            });
        }
        if let Some(requester) = requester {
            if vault_state.committee.member(requester).is_none() {
                return Err(Error::NotMember(*requester));
            }
            let cost = u32::try_from(points).unwrap_or(u32::MAX);
            if !state.limiter.take(&hex::encode(requester.0), cost, now) {
                return Err(Error::RateLimited);
            }
        }
        Ok(Serving {
            share,
            context: vault_state.committee.ecdh_context(),
        })
    }

    fn audit(
        &self,
        vault: &[u8; 32],
        requester: Option<&IdentityPublic>,
        purpose: EcdhPurpose,
        points: usize,
        outcome: &str,
    ) {
        let entry = AuditEntry {
            at: self.now(),
            requester: requester.map_or_else(|| "local".to_owned(), |r| hex::encode(r.0)),
            purpose: purpose_label(purpose).to_owned(),
            points: u32::try_from(points).unwrap_or(u32::MAX),
            outcome: outcome.to_owned(),
        };
        if let Err(error) = self.0.store.audit(vault, &entry) {
            tracing::error!(%error, "cannot write the audit log");
        }
    }

    async fn compute(serving: Serving, points: Vec<Point>) -> Result<Vec<Partial>> {
        tokio::task::spawn_blocking(move || {
            serving
                .share
                .partials(&points, &serving.context, &mut os_rng())
        })
        .await
        .map_err(|e| Error::Invalid(e.to_string()))?
        .map_err(Error::from)
    }

    /// Serves a peer's request, or refuses it (logged; the requester times out).
    pub(crate) async fn on_ecdh_request(
        &self,
        from: IdentityPublic,
        header: Header,
        request: EcdhRequest,
    ) {
        let vault = header.scope;
        let count = request.points.len();
        let result = async {
            let serving = self.admit(&vault, header.epoch, Some(&from), count).await?;
            let points = parse_points(&request.points)?;
            Self::compute(serving, points).await
        }
        .await;
        let partials = match result {
            Ok(partials) => {
                self.audit(&vault, Some(&from), request.purpose, count, "served");
                partials
            }
            Err(error) => {
                self.audit(
                    &vault,
                    Some(&from),
                    request.purpose,
                    count,
                    &format!("refused: {error}"),
                );
                return;
            }
        };
        let (peer, my_index) = {
            let state = self.0.state.lock().await;
            let Some(vault_state) = state.vaults.get(&vault) else {
                return;
            };
            let Some(peer) = vault_state
                .committee
                .peers_except(&self.identity())
                .into_iter()
                .find(|p| p.identity == from)
            else {
                return;
            };
            let Some(me) = vault_state.committee.member(&self.identity()) else {
                return;
            };
            (peer, me.index)
        };
        let response = Message::EcdhResponse(EcdhResponse::new(header.id, my_index, &partials));
        match self.sign(vault, header.epoch, response) {
            Ok((frame, _)) => {
                self.send(peer, frame).await;
            }
            Err(error) => tracing::error!(%error, "cannot sign an ECDH response"),
        }
    }

    /// Gathers `t_v` members' verified partials for `points` (this keeper's own plus peers'), for its member's daemon.
    ///
    /// # Errors
    /// [`Error::UnknownVault`], [`Error::WrongEpoch`], point errors, or [`Error::Unavailable`] if fewer than `t_v`
    /// members answered with valid partials in time.
    pub async fn ecdh_batch(
        &self,
        vault: [u8; 32],
        epoch: u32,
        purpose: EcdhPurpose,
        points: Vec<WirePoint>,
    ) -> Result<Vec<MemberPartials>> {
        let count = points.len();
        if count == 0 || count > ome_wire::limits::MAX_ECDH_POINTS {
            return Err(Error::Invalid("1..=4096 points per batch".into()));
        }
        let inputs = parse_points(&points)?;
        let serving = match self.admit(&vault, epoch, None, count).await {
            Ok(serving) => serving,
            Err(error) => {
                self.audit(&vault, None, purpose, count, &format!("refused: {error}"));
                return Err(error);
            }
        };
        let context = serving.context;
        let (committee, me) = {
            let state = self.0.state.lock().await;
            let committee = state
                .vaults
                .get(&vault)
                .ok_or(Error::UnknownVault)?
                .committee
                .clone();
            let me = committee
                .member(&self.identity())
                .ok_or(Error::UnknownVault)?
                .index;
            (committee, me)
        };
        let own = Self::compute(serving, inputs.clone()).await?;
        self.audit(&vault, None, purpose, count, "served");
        let threshold = committee.threshold();
        let mut gathered = vec![MemberPartials {
            member: me,
            partials: own,
        }];
        if threshold <= 1 {
            return Ok(gathered);
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let (frame, id) = self.sign(
            vault,
            epoch,
            Message::EcdhRequest(EcdhRequest { purpose, points }),
        )?;
        self.0
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, tx);
        let peers = committee.peers_except(&self.identity());
        for peer in peers {
            let keeper = self.clone();
            let frame = frame.clone();
            tokio::spawn(async move {
                keeper.send(peer, frame).await;
            });
        }

        self.collect(vault, &committee, context, inputs, rx, &mut gathered)
            .await;
        self.0
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
        if gathered.len() < threshold {
            return Err(Error::Unavailable {
                needed: threshold,
                received: gathered.len(),
            });
        }
        Ok(gathered)
    }

    /// Adds verified peer responses to `gathered` until it holds `t_v` members or the timeout passes.
    async fn collect(
        &self,
        vault: [u8; 32],
        committee: &Committee,
        context: EcdhContext,
        inputs: Vec<Point>,
        mut rx: mpsc::UnboundedReceiver<(IdentityPublic, EcdhResponse)>,
        gathered: &mut Vec<MemberPartials>,
    ) {
        let threshold = committee.threshold();
        let deadline = Instant::now() + self.0.config.ecdh_timeout;
        let inputs = Arc::new(inputs);
        let mut answered: BTreeSet<MemberIndex> = BTreeSet::new();
        while gathered.len() < threshold {
            let Ok(Some((from, response))) = tokio::time::timeout_at(deadline, rx.recv()).await
            else {
                break;
            };
            let Some(member) = committee.member(&from) else {
                continue;
            };
            if member.index != response.member || !answered.insert(member.index) {
                continue;
            }
            let Some(public) = committee.address_share(member.index) else {
                continue;
            };
            let inputs = Arc::clone(&inputs);
            let valid =
                tokio::task::spawn_blocking(move || verify(&response, &inputs, &public, &context))
                    .await
                    .unwrap_or(None);
            match valid {
                Some(partials) => gathered.push(MemberPartials {
                    member: member.index,
                    partials,
                }),
                None => self.emit(Event::InvalidPartials {
                    vault: hex::encode(vault),
                    member: member.index,
                }),
            }
        }
    }
}

/// The response's partials if every one carries a valid proof for the matching input.
fn verify(
    response: &EcdhResponse,
    inputs: &[Point],
    public: &Point,
    context: &EcdhContext,
) -> Option<Vec<Partial>> {
    let partials = response.partials().ok()?;
    if partials.len() != inputs.len() {
        return None;
    }
    partials
        .iter()
        .zip(inputs)
        .all(|(partial, input)| ecdh::verify_partial(partial, input, public, context).is_ok())
        .then_some(partials)
}
