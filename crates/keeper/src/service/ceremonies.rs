use std::time::Duration;

use ome_api::keeper::{CeremonyKind, CeremonyStatus, KeeperEvent};
use ome_crypto::threshold::shamir::MemberIndex;
use ome_net::{Inbound, Peer};
use ome_protocol::vault::Committee as Viewers;
use ome_wire::{
    IdentityPublic, Message, decode,
    messages::ceremony::{Echo, Manifest, Member},
    types::{Bytes64, WireField, WirePoint, field},
};
use serde::{Deserialize, Serialize};

use crate::keeper::{Keeper, State};
use crate::{
    Error, Result,
    ceremony::{
        Failure, Outbox, Participant, Status, Target,
        dkg::{Dkg, DkgParams},
        reshare::{Reshare, ReshareParams},
    },
    committee::Committee,
    custody::{ShareSlot, StoredShare},
    random::os_rng,
    store::from_json,
};

/// A finished DKG, as reported to the daemon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DkgResult {
    /// This keeper's member index.
    pub index: MemberIndex,
    /// Viewing threshold.
    pub threshold: u32,
    /// Group key `Ivpk`.
    pub group_key: WirePoint,
    /// Verification shares in member order.
    pub verification_shares: Vec<WirePoint>,
    /// Transcript every member confirmed.
    pub transcript: Bytes64,
}

/// A member of the committee a reshare creates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewMember {
    /// Keeper identity.
    pub identity: IdentityPublic,
    /// Keeper transport key.
    pub transport: [u8; 32],
    /// Keeper addresses.
    pub addresses: Vec<String>,
    /// The member's signing key.
    pub signer_key: WirePoint,
}

/// What a member's owner asks its keeper to reshare.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReshareRequest {
    /// Session id (agreed out of band).
    pub session: [u8; 32],
    /// Vault.
    pub vault: [u8; 32],
    /// The current manifest, for a joining member whose keeper does not hold the vault (verified by its daemon
    /// against the on-chain committee commitment).
    pub current: Option<Manifest>,
    /// Exactly `t_v` current members who deal.
    pub dealers: Vec<MemberIndex>,
    /// New viewing threshold.
    pub new_threshold: u32,
    /// New spending threshold `k`.
    pub new_spending_threshold: u32,
    /// New committee, in slot order.
    pub new_members: Vec<NewMember>,
}

/// A finished reshare, as reported to the daemon (the new verification shares go into `rotate_membership`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReshareResult {
    /// The new committee's commitment, as `rotate_membership` will store it. Every participant computes it from the
    /// dealers' commitments, so a dealer that leaves can still recognise the rotation it took part in.
    pub committee_commitment: WireField,
    /// This keeper's index in the new committee, if it is a member.
    pub new_index: Option<MemberIndex>,
    /// The new committee's manifest (epoch fixed at activation); absent for dealers leaving the committee.
    pub manifest: Option<Manifest>,
    /// Transcript every new member confirmed.
    pub transcript: Option<Bytes64>,
}

/// Status of a ceremony.
#[derive(Clone, Debug, Serialize)]
pub struct CeremonyInfo {
    /// `dkg` or `reshare`.
    pub kind: String,
    /// `running`, `completed`, `aborted` or `activated`.
    pub status: String,
    /// Why it stopped, if aborted.
    pub failure: Option<Failure>,
    /// Result (JSON), once completed.
    pub result: Option<serde_json::Value>,
}

impl Keeper {
    /// Starts (or joins) a DKG. Every member's owner calls this with the same parameters; a keeper never joins a
    /// ceremony its owner did not start.
    ///
    /// # Errors
    /// [`Error::CeremonyExists`] or parameter errors.
    pub async fn start_dkg(&self, params: DkgParams) -> Result<()> {
        let session = params.session;
        let now = self.now();
        let mut state = self.0.state.lock().await;
        if state.dkgs.contains_key(&session) || state.reshares.contains_key(&session) {
            return Err(Error::CeremonyExists);
        }
        let (dkg, outbox) = Dkg::start(
            params.clone(),
            &self.identity(),
            now,
            self.0.config.round_timeout_secs,
            &mut os_rng(),
        )?;
        self.0
            .store
            .create_ceremony(&session, "dkg", None, &params, now)?;
        state.dkgs.insert(session, dkg);
        self.refresh_allowlist(&state);
        let sends = self.resolve(&state, session, outbox);
        drop(state);
        self.emit_ceremony(session, "dkg", "running");
        self.dispatch(sends);
        self.replay_early(session).await;
        Ok(())
    }

    /// Starts (or joins) a reshare of `request.vault`.
    ///
    /// # Errors
    /// [`Error::UnknownVault`] without a current manifest, [`Error::CeremonyExists`], or parameter errors.
    pub async fn start_reshare(&self, request: ReshareRequest) -> Result<()> {
        let session = request.session;
        let now = self.now();
        let mut state = self.0.state.lock().await;
        if state.dkgs.contains_key(&session) || state.reshares.contains_key(&session) {
            return Err(Error::CeremonyExists);
        }
        let committee = match state.vaults.get(&request.vault) {
            Some(vault) => vault.committee.clone(),
            None => Committee::new(request.current.clone().ok_or(Error::UnknownVault)?)?,
        };
        if committee.vault_bytes() != request.vault {
            return Err(Error::Invalid("manifest is for another vault".into()));
        }
        let manifest = &committee.manifest;
        let params = ReshareParams {
            session,
            vault: request.vault,
            old_epoch: manifest.epoch,
            old_threshold: manifest.viewing_threshold,
            old_members: manifest.members.iter().map(participant).collect(),
            old_verification_shares: manifest
                .members
                .iter()
                .map(|m| m.verification_share)
                .collect(),
            dealers: request.dealers.clone(),
            new_threshold: request.new_threshold,
            new_members: request
                .new_members
                .iter()
                .map(|m| Participant {
                    identity: m.identity,
                    transport: m.transport,
                    addresses: m.addresses.clone(),
                })
                .collect(),
        };
        let current = match committee.member(&self.identity()) {
            Some(member) if request.dealers.contains(&member.index) => Some(StoredShare::load(
                &self.0.secrets,
                ShareSlot::Active(request.vault, manifest.epoch),
            )?),
            _ => None,
        };
        let mut rng = os_rng();
        let (reshare, mut outbox) = Reshare::start(
            params,
            &self.identity(),
            current.as_ref(),
            now,
            self.0.config.round_timeout_secs,
            &mut rng,
        )?;
        drop(current);
        outbox
            .0
            .extend(reshare.seal_sub_shares(&self.0.keys.transport, &mut rng)?.0);
        self.0
            .store
            .create_ceremony(&session, "reshare", Some(&request.vault), &request, now)?;
        state.reshares.insert(session, reshare);
        self.refresh_allowlist(&state);
        let sends = self.resolve(&state, session, outbox);
        self.settle(&mut state, session);
        drop(state);
        self.emit_ceremony(session, "reshare", "running");
        self.dispatch(sends);
        self.replay_early(session).await;
        Ok(())
    }

    /// Status of a ceremony (from memory while running, from the store afterwards).
    ///
    /// # Errors
    /// [`Error::UnknownCeremony`].
    pub fn ceremony(&self, session: [u8; 32]) -> Result<CeremonyInfo> {
        let row = self
            .0
            .store
            .ceremony(&session)?
            .ok_or(Error::UnknownCeremony)?;
        let result = row
            .result
            .as_deref()
            .map(from_json::<serde_json::Value>)
            .transpose()?;
        let failure = if row.status == "aborted" {
            result.clone().and_then(|r| serde_json::from_value(r).ok())
        } else {
            None
        };
        Ok(CeremonyInfo {
            kind: row.kind,
            status: row.status,
            failure,
            result: if failure_like(result.as_ref()) {
                None
            } else {
                result
            },
        })
    }

    pub(crate) async fn on_ceremony_message(
        &self,
        from: IdentityPublic,
        session: [u8; 32],
        body: Message,
    ) {
        let now = self.now();
        let mut state = self.0.state.lock().await;
        let outbox = if let Some(dkg) = state.dkgs.get_mut(&session) {
            dkg.handle(&from, body, &self.0.keys.transport, now, &mut os_rng())
        } else if let Some(reshare) = state.reshares.get_mut(&session) {
            reshare.handle(&from, body, &self.0.keys.transport, now)
        } else {
            return;
        };
        let sends = self.resolve(&state, session, outbox);
        self.settle(&mut state, session);
        drop(state);
        self.dispatch(sends);
    }

    /// Advances every ceremony's clock.
    pub(crate) async fn tick(&self) {
        let now = self.now();
        let mut state = self.0.state.lock().await;
        let mut sends = Vec::new();
        let sessions: Vec<[u8; 32]> = state
            .dkgs
            .keys()
            .chain(state.reshares.keys())
            .copied()
            .collect();
        for session in sessions {
            let outbox = if let Some(dkg) = state.dkgs.get_mut(&session) {
                dkg.tick(now)
            } else if let Some(reshare) = state.reshares.get_mut(&session) {
                reshare.tick(now)
            } else {
                continue;
            };
            sends.extend(self.resolve(&state, session, outbox));
            self.settle(&mut state, session);
        }
        let before = state.lingering.len();
        state.lingering.retain(|_, (until, _)| *until > now);
        if state.lingering.len() != before {
            self.refresh_allowlist(&state);
        }
        drop(state);
        self.dispatch(sends);
        self.expire_pending(now);
        self.periodic(now).await;
    }

    /// Persists a finished ceremony and drops its in-memory state.
    fn settle(&self, state: &mut State, session: [u8; 32]) {
        let now = self.now();
        let finished = participants_of(state, session).map(|(participants, _)| participants);
        if let Some(dkg) = state.dkgs.get_mut(&session) {
            match dkg.status().clone() {
                Status::Running => return,
                Status::Completed => {
                    let outcome = dkg
                        .take_result()
                        .ok_or(Error::Invalid("no DKG result".into()))
                        .and_then(|share| {
                            StoredShare::save(&self.0.secrets, ShareSlot::Dkg(session), &share)?;
                            Ok(DkgResult {
                                index: share.index(),
                                threshold: u32::try_from(share.threshold()).unwrap_or(u32::MAX),
                                group_key: WirePoint::from(share.group_public_key()),
                                verification_shares: share
                                    .verification_shares()?
                                    .iter()
                                    .map(WirePoint::from)
                                    .collect(),
                                transcript: Bytes64::from_array(share.transcript()),
                            })
                        });
                    self.record(
                        session,
                        "dkg",
                        outcome.map(|r| ("completed", serde_json::to_value(r).ok())),
                        now,
                    );
                }
                Status::Aborted(failure) => self.record(
                    session,
                    "dkg",
                    Ok(("aborted", serde_json::to_value(failure).ok())),
                    now,
                ),
            }
            state.dkgs.remove(&session);
        } else if let Some(reshare) = state.reshares.get_mut(&session) {
            match reshare.status().clone() {
                Status::Running => return,
                Status::Completed => {
                    let outcome = self.finish_reshare(session, reshare);
                    self.record(
                        session,
                        "reshare",
                        outcome.map(|r| ("completed", serde_json::to_value(r).ok())),
                        now,
                    );
                }
                Status::Aborted(failure) => self.record(
                    session,
                    "reshare",
                    Ok(("aborted", serde_json::to_value(failure).ok())),
                    now,
                ),
            }
            state.reshares.remove(&session);
        } else {
            return;
        }
        if let Some(participants) = finished {
            let until = now + self.0.config.round_timeout_secs;
            state.lingering.insert(session, (until, participants));
        }
        self.refresh_allowlist(state);
    }

    fn finish_reshare(&self, session: [u8; 32], reshare: &mut Reshare) -> Result<ReshareResult> {
        let committee = Viewers::new(
            reshare.params().new_threshold,
            reshare.new_verification_shares()?,
        )?;
        let committee_commitment = field(&committee.commitment());
        let Some(share) = reshare.take_result() else {
            return Ok(ReshareResult {
                committee_commitment,
                new_index: None,
                manifest: None,
                transcript: None,
            });
        };
        let row = self
            .0
            .store
            .ceremony(&session)?
            .ok_or(Error::UnknownCeremony)?;
        let request: ReshareRequest = from_json(&row.params)?;
        let old = match self.0.store.latest_manifest(&request.vault)? {
            Some((manifest, _)) => manifest,
            None => request.current.clone().ok_or(Error::UnknownVault)?,
        };
        let shares = share.verification_shares()?;
        if committee.shares() != shares.as_slice() {
            return Err(Error::Invalid(
                "the combined share does not match the dealers' commitments".into(),
            ));
        }
        let manifest = Manifest {
            protocol_version: old.protocol_version,
            vault: request.vault,
            // Fixed when the on-chain config activates the committee.
            epoch: 0,
            threshold: request.new_spending_threshold,
            viewing_threshold: request.new_threshold,
            preaddress: old.preaddress,
            group_key: WirePoint::from(share.group_public_key()),
            members: request
                .new_members
                .iter()
                .zip(&shares)
                .zip(1..)
                .map(|((m, y), index)| Member {
                    index,
                    signer_key: m.signer_key,
                    verification_share: WirePoint::from(y),
                    identity: m.identity,
                    transport: m.transport,
                    addresses: m.addresses.clone(),
                })
                .collect(),
        };
        if manifest.group_key != old.group_key {
            return Err(Error::Invalid("reshare changed the group key".into()));
        }
        Committee::new(manifest.clone())?;
        StoredShare::save(&self.0.secrets, ShareSlot::Pending(session), &share)?;
        Ok(ReshareResult {
            committee_commitment,
            new_index: reshare.new_index(),
            manifest: Some(manifest),
            transcript: Some(Bytes64::from_array(share.transcript())),
        })
    }

    fn record(
        &self,
        session: [u8; 32],
        kind: &str,
        outcome: Result<(&str, Option<serde_json::Value>)>,
        now: u64,
    ) {
        let (status, result) = match outcome {
            Ok(outcome) => outcome,
            Err(error) => (
                "aborted",
                serde_json::to_value(Failure {
                    reason: format!("cannot store the result: {error}"),
                    blamed: Vec::new(),
                })
                .ok(),
            ),
        };
        if let Err(error) = self
            .0
            .store
            .update_ceremony(&session, status, result.as_ref(), now)
        {
            tracing::error!(%error, "cannot record a ceremony");
        }
        self.emit_ceremony(session, kind, status);
    }

    pub(super) fn emit_ceremony(&self, session: [u8; 32], kind: &str, status: &str) {
        self.emit(KeeperEvent::Ceremony {
            session: session.into(),
            kind: ceremony_kind(kind),
            status: ceremony_status(status),
        });
    }

    /// Signs an outbox for `session` and resolves its targets to peers.
    fn resolve(&self, state: &State, session: [u8; 32], outbox: Outbox) -> Vec<(Peer, Vec<u8>)> {
        let Some((participants, epoch)) = participants_of(state, session) else {
            return Vec::new();
        };
        let me = self.identity();
        let mut sends = Vec::new();
        for (target, message) in outbox.0 {
            let Ok((frame, _)) = self.sign(session, epoch, message) else {
                continue;
            };
            let mut recipients: Vec<Peer> = participants
                .iter()
                .filter(|p| {
                    p.identity != me && (target == Target::All || target == Target::One(p.identity))
                })
                .map(Peer::from)
                .collect();
            recipients.dedup_by_key(|p| p.identity);
            for peer in recipients {
                sends.push((peer, frame.clone()));
            }
        }
        sends
    }

    /// Sends ceremony messages, retrying each until delivered or the round would have timed out (a member may not
    /// have started the session yet, or be briefly unreachable).
    fn dispatch(&self, sends: Vec<(Peer, Vec<u8>)>) {
        let give_up = Duration::from_secs(self.0.config.round_timeout_secs);
        for (peer, frame) in sends {
            let keeper = self.clone();
            tokio::spawn(async move {
                keeper.send_with_retry(peer, frame, give_up).await;
            });
        }
    }

    /// Processes messages that arrived before the session started.
    async fn replay_early(&self, session: [u8; 32]) {
        let backlog = self
            .0
            .state
            .lock()
            .await
            .early
            .remove(&session)
            .unwrap_or_default();
        for (_, channel, bytes) in backlog {
            Box::pin(self.on_inbound(Inbound { channel, bytes })).await;
        }
    }

    /// Forwards a dealer's signed broadcast to the other participants, once per session and dealer, so that every
    /// participant sees every version a dealer sent (two different signed versions prove equivocation).
    pub(crate) async fn echo(&self, session: [u8; 32], dealer: IdentityPublic, frame: &[u8]) {
        let me = self.identity();
        let sends = {
            let mut state = self.0.state.lock().await;
            let Some((participants, epoch)) = participants_of(&state, session) else {
                return;
            };
            if !state.echoed.insert((session, dealer)) {
                return;
            }
            let Ok((signed, _)) = self.sign(
                session,
                epoch,
                Message::Echo(Echo {
                    frame: frame.to_vec(),
                }),
            ) else {
                return;
            };
            let mut peers: Vec<Peer> = participants
                .iter()
                .filter(|p| p.identity != me && p.identity != dealer)
                .map(Peer::from)
                .collect();
            peers.dedup_by_key(|p| p.identity);
            peers.into_iter().map(|p| (p, signed.clone())).collect()
        };
        self.dispatch(sends);
    }

    /// Feeds an echoed dealer broadcast to its ceremony as if the dealer had sent it: its signature proves its origin.
    pub(crate) async fn on_echo(&self, session: [u8; 32], echo: &Echo) {
        let Ok(inner) = decode(&echo.frame) else {
            return;
        };
        if inner.verify().is_err() || inner.header.scope != session {
            return;
        }
        if matches!(inner.body, Message::DkgRound1(_) | Message::ReshareDeal(_)) {
            Box::pin(self.on_ceremony_message(inner.signer, session, inner.body)).await;
        }
    }
}

/// Participants and envelope epoch of a running ceremony.
fn participants_of(state: &State, session: [u8; 32]) -> Option<(Vec<Participant>, u32)> {
    if let Some(dkg) = state.dkgs.get(&session) {
        Some((dkg.params().members.clone(), 0))
    } else {
        state.reshares.get(&session).map(|reshare| {
            let params = reshare.params();
            (
                params
                    .old_members
                    .iter()
                    .chain(&params.new_members)
                    .cloned()
                    .collect(),
                params.old_epoch,
            )
        })
    }
}

fn participant(member: &Member) -> Participant {
    Participant {
        identity: member.identity,
        transport: member.transport,
        addresses: member.addresses.clone(),
    }
}

fn failure_like(value: Option<&serde_json::Value>) -> bool {
    value.is_some_and(|v| v.get("reason").is_some() && v.get("blamed").is_some())
}

/// The API's name for a stored ceremony kind label.
pub(crate) fn ceremony_kind(label: &str) -> CeremonyKind {
    if label == "dkg" {
        CeremonyKind::Dkg
    } else {
        CeremonyKind::Reshare
    }
}

/// The API's name for a stored ceremony status label.
pub(crate) fn ceremony_status(label: &str) -> CeremonyStatus {
    match label {
        "running" => CeremonyStatus::Running,
        "completed" => CeremonyStatus::Completed,
        "activated" => CeremonyStatus::Activated,
        "expired" => CeremonyStatus::Expired,
        _ => CeremonyStatus::Aborted,
    }
}
