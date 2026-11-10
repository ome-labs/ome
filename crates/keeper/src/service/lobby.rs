//! Vault-founding lobbies. The founder's keeper hosts a lobby; co-founders' keepers join it with an invite, receive the
//! roster as it changes, and receive the plan (DKG session, thresholds, salt, members in slot order and the daemon's
//! protocol secrets sealed to each member) when the founder starts it. A keeper never starts the DKG itself: each
//! member's daemon reads the plan and calls `ceremony.dkg`. See `architecture/keeper.md`.

use std::time::Duration;

use ome_api::keeper::{KeeperEvent, LobbyChange, LobbyChangeKind, LobbyRole, LobbyStatus};
use ome_crypto::{
    Point,
    sealing::envelope::{Envelope, TransportPublic},
};
use ome_net::{Channel, Peer};
use ome_protocol::lobby::{join_proof, open_secrets, plan_version, seal_secrets, verify_join};
use ome_wire::{
    IdentityPublic, Message, SignedMessage,
    limits::{MAX_LOBBY_SECRETS_BYTES, MAX_MEMBERS, MAX_NAME_BYTES},
    messages::lobby::{
        Invite, JoinRequest, JoinSecret, KeeperContact, LobbyJoin, LobbyLeave, LobbyMember,
        LobbyPlan, LobbyRemoved, LobbyState, SealedSecrets, scope,
    },
    types::{WireField, WirePoint, parse_field},
    version::SUPPORTED,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    Error, Result,
    ceremony::Participant,
    keeper::{Keeper, State},
    random::{os_rng, random_bytes},
    store::from_json,
};

/// How long a guest retries its join, and the host its roster and plan sends.
const LOBBY_SEND_WINDOW: Duration = Duration::from_secs(30);

/// A lobby role as the store names it.
const fn role_label(role: LobbyRole) -> &'static str {
    match role {
        LobbyRole::Host => "host",
        LobbyRole::Guest => "guest",
    }
}

/// A lobby status as the store and messages name it.
const fn status_label(status: LobbyStatus) -> &'static str {
    match status {
        LobbyStatus::Open => "open",
        LobbyStatus::Started => "started",
        LobbyStatus::Closed => "closed",
    }
}

/// What a keeper keeps about a lobby (public data only; the invite secret is in custody).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LobbyRecord {
    pub(crate) lobby: [u8; 16],
    pub(crate) role: LobbyRole,
    pub(crate) status: LobbyStatus,
    pub(crate) name: String,
    pub(crate) host_name: String,
    pub(crate) threshold: u8,
    pub(crate) viewing_threshold: u8,
    pub(crate) network: String,
    pub(crate) host: KeeperContact,
    /// This keeper's own entry.
    pub(crate) me: LobbyMember,
    /// Roster, host first (the plan's members once started).
    pub(crate) members: Vec<LobbyMember>,
    pub(crate) revision: u32,
    pub(crate) plan: Option<LobbyPlan>,
    /// How many members the vault will have (0 for a lobby stored before lobbies had a size).
    #[serde(default)]
    pub(crate) expected: u8,
    /// The host removed this keeper's member.
    #[serde(default)]
    pub(crate) removed: bool,
}

impl LobbyRecord {
    /// Peers this keeper talks to for the lobby: the host for a guest, every guest for the host.
    fn peers(&self, me: &IdentityPublic) -> Vec<Participant> {
        match self.role {
            LobbyRole::Guest => vec![participant(&self.host)],
            LobbyRole::Host => self
                .members
                .iter()
                .filter(|m| &m.keeper.identity != me)
                .map(|m| participant(&m.keeper))
                .collect(),
        }
    }
}

/// What the daemon asks for when it opens a lobby.
#[derive(Clone, Debug)]
pub struct LobbyOpen {
    /// Vault name.
    pub name: String,
    /// Proposed spending threshold.
    pub threshold: u8,
    /// Proposed viewing threshold.
    pub viewing_threshold: u8,
    /// How many members the vault will have, host included.
    pub members: u8,
    /// Opaque network label.
    pub network: String,
    /// The host's display name.
    pub member_name: String,
    /// The host's Aztec account.
    pub member_account: WireField,
    /// The host's signing key.
    pub signer_key: WirePoint,
}

/// A lobby's plan as reported to the daemon.
#[derive(Clone, Debug)]
pub struct LobbyPlanInfo {
    /// Protocol version the vault is founded on.
    pub protocol_version: u16,
    /// DKG session.
    pub session: [u8; 32],
    /// Spending threshold.
    pub threshold: u8,
    /// Viewing threshold.
    pub viewing_threshold: u8,
    /// Vault salt.
    pub salt: WireField,
    /// This member's protocol secrets (only when asked for).
    pub secrets: Option<Zeroizing<Vec<u8>>>,
}

/// A lobby as reported to the daemon.
#[derive(Clone, Debug)]
pub struct LobbyInfo {
    /// Lobby id.
    pub lobby: [u8; 16],
    /// Role.
    pub role: LobbyRole,
    /// Status.
    pub status: LobbyStatus,
    /// Vault name.
    pub name: String,
    /// Host's display name.
    pub host_name: String,
    /// Proposed spending threshold.
    pub threshold: u8,
    /// Proposed viewing threshold.
    pub viewing_threshold: u8,
    /// Network label.
    pub network: String,
    /// The host's keeper.
    pub host: KeeperContact,
    /// Roster, host first.
    pub members: Vec<LobbyMember>,
    /// The plan, once started.
    pub plan: Option<LobbyPlanInfo>,
    /// The invite, while this keeper hosts the lobby and it is open.
    pub invite: Option<String>,
    /// How many members the vault will have (0 when the lobby predates sizes).
    pub expected: u8,
    /// The host removed this keeper's member.
    pub removed: bool,
}

fn participant(contact: &KeeperContact) -> Participant {
    Participant {
        identity: contact.identity,
        transport: contact.transport,
        addresses: contact.addresses.clone(),
    }
}

fn secret_name(lobby: &[u8; 16]) -> String {
    format!("lobby-{}", hex::encode(lobby))
}

fn check_name(field: &str, value: &str) -> Result<()> {
    if value.len() > MAX_NAME_BYTES {
        return Err(Error::Invalid(format!("{field} is longer than 64 bytes")));
    }
    Ok(())
}

fn check_point(point: &WirePoint) -> Result<()> {
    Point::try_from(point)?;
    Ok(())
}

/// `1 ≤ k ≤ n` and `2 ≤ t_v ≤ n`.
fn check_thresholds(threshold: u8, viewing_threshold: u8, members: usize) -> Result<()> {
    let n = u8::try_from(members).map_err(|_| Error::Invalid("too many members".into()))?;
    if !(1..=n).contains(&threshold) {
        return Err(Error::Invalid(format!(
            "threshold {threshold} is not between 1 and {n}"
        )));
    }
    if !(2..=n).contains(&viewing_threshold) {
        return Err(Error::Invalid(format!(
            "viewing threshold {viewing_threshold} is not between 2 and {n}"
        )));
    }
    Ok(())
}

fn index_of(members: &[LobbyMember], identity: &IdentityPublic) -> Option<u8> {
    members
        .iter()
        .position(|m| &m.keeper.identity == identity)
        .and_then(|i| u8::try_from(i + 1).ok())
}

pub(crate) fn is_lobby_message(body: &Message) -> bool {
    matches!(
        body,
        Message::LobbyJoin(_)
            | Message::LobbyState(_)
            | Message::LobbyPlan(_)
            | Message::LobbyLeave(_)
            | Message::LobbyRemoved(_)
    )
}

fn lobby_of(body: &Message) -> Option<[u8; 16]> {
    match body {
        Message::LobbyJoin(m) => Some(m.lobby),
        Message::LobbyState(m) => Some(m.lobby),
        Message::LobbyPlan(m) => Some(m.lobby),
        Message::LobbyLeave(m) => Some(m.lobby),
        Message::LobbyRemoved(m) => Some(m.lobby),
        _ => None,
    }
}

impl Keeper {
    /// Loads stored lobbies (at start).
    pub(crate) fn stored_lobbies(&self) -> Result<Vec<LobbyRecord>> {
        self.0
            .store
            .lobbies()?
            .iter()
            .map(|text| from_json(text))
            .collect()
    }

    /// A join request for a vault that already exists: `name`, `account` and `signer_key` with this keeper as the
    /// newcomer's, signed by this keeper's identity, in its string form.
    ///
    /// # Errors
    /// Bounds errors.
    pub fn join_request(
        &self,
        name: String,
        account: WireField,
        signer_key: WirePoint,
    ) -> Result<String> {
        let member = LobbyMember {
            name,
            account,
            signer_key,
            keeper: self.contact(),
        };
        Ok(JoinRequest::sign(&self.0.keys.identity, member)?.encode()?)
    }

    fn contact(&self) -> KeeperContact {
        KeeperContact {
            identity: self.identity(),
            transport: self.transport_key(),
            versions: SUPPORTED,
            addresses: self.addresses().to_vec(),
        }
    }

    fn persist_lobby(&self, record: &LobbyRecord) -> Result<()> {
        self.0.store.put_lobby(
            &record.lobby,
            role_label(record.role),
            status_label(record.status),
            record,
            self.now(),
        )
    }

    fn emit_lobby(
        &self,
        record: &LobbyRecord,
        kind: Option<LobbyChangeKind>,
        member: Option<IdentityPublic>,
    ) {
        self.emit(KeeperEvent::Lobby {
            lobby: record.lobby.into(),
            status: record.status,
            members: u32::try_from(record.members.len()).unwrap_or(u32::MAX),
            change: kind.map(|kind| LobbyChange {
                kind,
                member: member.map(|m| m.0.into()),
            }),
        });
    }

    /// Keeps a lobby's peers reachable for one round timeout after it stops accepting (late retries, the DKG start).
    fn linger_lobby(&self, state: &mut State, record: &LobbyRecord) {
        let until = self.now() + self.0.config.round_timeout_secs;
        state.lingering.insert(
            scope(&record.lobby),
            (until, record.peers(&self.identity())),
        );
    }

    /// Sends a lobby frame, retrying for a while and falling back to the peer's relay mailbox.
    fn send_lobby_frame(&self, peers: Vec<Participant>, frame: &[u8]) {
        for peer in peers {
            let keeper = self.clone();
            let peer = Peer::from(&peer);
            let frame = frame.to_vec();
            tokio::spawn(async move {
                if !keeper
                    .send_with_retry(peer.clone(), frame.clone(), LOBBY_SEND_WINDOW)
                    .await
                    && let Err(error) = keeper.deposit(&peer, &frame).await
                {
                    tracing::debug!(%error, "lobby mailbox deposit failed");
                }
            });
        }
    }

    /// Opens a lobby hosted by this keeper (member 1). Returns its id and invite.
    ///
    /// # Errors
    /// [`Error::Invalid`] for bad names, thresholds or signing key; custody errors.
    pub async fn open_lobby(&self, request: LobbyOpen) -> Result<([u8; 16], String)> {
        check_name("name", &request.name)?;
        check_name("member name", &request.member_name)?;
        check_name("network", &request.network)?;
        check_point(&request.signer_key)?;
        parse_field(&request.member_account)?;
        if !(2..=MAX_MEMBERS).contains(&usize::from(request.members)) {
            return Err(Error::Invalid(format!(
                "a vault has 2 to {MAX_MEMBERS} members"
            )));
        }
        check_thresholds(
            request.threshold,
            request.viewing_threshold,
            usize::from(request.members),
        )?;
        let lobby: [u8; 16] = random_bytes();
        let invite = Invite {
            lobby,
            secret: JoinSecret(random_bytes()),
            host: self.contact(),
            host_name: request.member_name.clone(),
            name: request.name.clone(),
            threshold: request.threshold,
            viewing_threshold: request.viewing_threshold,
            members: request.members,
            network: request.network.clone(),
        };
        let text = invite.encode()?;
        self.0.secrets.put(&secret_name(&lobby), &invite.secret.0)?;
        let me = LobbyMember {
            name: request.member_name,
            account: request.member_account,
            signer_key: request.signer_key,
            keeper: invite.host.clone(),
        };
        let record = LobbyRecord {
            lobby,
            role: LobbyRole::Host,
            status: LobbyStatus::Open,
            name: invite.name.clone(),
            host_name: invite.host_name.clone(),
            threshold: invite.threshold,
            viewing_threshold: invite.viewing_threshold,
            network: invite.network.clone(),
            host: invite.host.clone(),
            members: vec![me.clone()],
            me,
            revision: 0,
            plan: None,
            expected: invite.members,
            removed: false,
        };
        self.persist_lobby(&record)?;
        let mut state = self.0.state.lock().await;
        state.lobbies.insert(lobby, record.clone());
        self.refresh_allowlist(&state);
        drop(state);
        self.emit_lobby(&record, Some(LobbyChangeKind::Opened), None);
        Ok((lobby, text))
    }

    /// Joins the lobby of `invite` as `member_name` with `signer_key`: records it, then sends the join to the host
    /// until the host's keeper acknowledges it (about 30 s).
    ///
    /// # Errors
    /// [`Error::Invalid`] for a bad invite or one this keeper hosts or already finished; [`Error::Transport`] if the
    /// host never acknowledged.
    pub async fn join_lobby(
        &self,
        invite: &str,
        member_name: String,
        member_account: WireField,
        signer_key: WirePoint,
    ) -> Result<[u8; 16]> {
        let invite = Invite::decode(invite)?;
        check_name("member name", &member_name)?;
        check_point(&signer_key)?;
        parse_field(&member_account)?;
        if invite.host.identity == self.identity() {
            return Err(Error::Invalid("this keeper hosts the lobby".into()));
        }
        let lobby = invite.lobby;
        let me = LobbyMember {
            name: member_name,
            account: member_account,
            signer_key,
            keeper: self.contact(),
        };
        let record = {
            let mut state = self.0.state.lock().await;
            let record = match state.lobbies.get(&lobby) {
                Some(existing)
                    if existing.role != LobbyRole::Guest
                        || existing.status != LobbyStatus::Open =>
                {
                    return Err(Error::Invalid(format!(
                        "lobby is already {}",
                        status_label(existing.status)
                    )));
                }
                Some(existing) => LobbyRecord {
                    me: me.clone(),
                    ..existing.clone()
                },
                None => LobbyRecord {
                    lobby,
                    role: LobbyRole::Guest,
                    status: LobbyStatus::Open,
                    name: invite.name.clone(),
                    host_name: invite.host_name.clone(),
                    threshold: invite.threshold,
                    viewing_threshold: invite.viewing_threshold,
                    network: invite.network.clone(),
                    host: invite.host.clone(),
                    me: me.clone(),
                    members: Vec::new(),
                    revision: 0,
                    plan: None,
                    expected: invite.members,
                    removed: false,
                },
            };
            self.0.secrets.put(&secret_name(&lobby), &invite.secret.0)?;
            self.persist_lobby(&record)?;
            state.lobbies.insert(lobby, record.clone());
            self.refresh_allowlist(&state);
            record
        };
        let proof = join_proof(&invite.secret, &lobby, &me.keeper);
        let (frame, _) = self.sign(
            scope(&lobby),
            0,
            Message::LobbyJoin(LobbyJoin {
                lobby,
                proof,
                member: me,
            }),
        )?;
        self.emit_lobby(&record, Some(LobbyChangeKind::Opened), None);
        if self
            .send_with_retry(
                Peer::from(&participant(&invite.host)),
                frame,
                LOBBY_SEND_WINDOW,
            )
            .await
        {
            Ok(lobby)
        } else {
            Err(Error::Transport(
                "the lobby host did not acknowledge the join".into(),
            ))
        }
    }

    /// Starts a lobby this keeper hosts: fixes the plan (a fresh DKG session, the roster in order, `threshold`,
    /// `viewing_threshold` and `salt`), seals `secrets` to every member and sends the plan to the guests. Admission
    /// closes.
    ///
    /// # Errors
    /// [`Error::Invalid`] unless this keeper hosts the open lobby and the parameters fit the roster.
    pub async fn start_lobby(
        &self,
        lobby: [u8; 16],
        threshold: u8,
        viewing_threshold: u8,
        salt: WireField,
        secrets: &[u8],
    ) -> Result<LobbyInfo> {
        if secrets.len() > MAX_LOBBY_SECRETS_BYTES {
            return Err(Error::Invalid("secrets are larger than 1 KiB".into()));
        }
        parse_field(&salt)?;
        let mut state = self.0.state.lock().await;
        let record = state
            .lobbies
            .get(&lobby)
            .ok_or_else(|| Error::Invalid("unknown lobby".into()))?;
        if record.role != LobbyRole::Host || record.status != LobbyStatus::Open {
            return Err(Error::Invalid(
                "only the host can start an open lobby".into(),
            ));
        }
        if record.expected > 0 && record.members.len() != usize::from(record.expected) {
            return Err(Error::Incomplete {
                joined: record.members.len(),
                expected: usize::from(record.expected),
            });
        }
        check_thresholds(threshold, viewing_threshold, record.members.len())?;
        let session: [u8; 32] = random_bytes();
        let mut rng = os_rng();
        let sealed = record
            .members
            .iter()
            .zip(1u8..)
            .map(|(member, index)| {
                let envelope = seal_secrets(
                    &session,
                    u32::from(index),
                    &self.0.keys.transport,
                    &TransportPublic(member.keeper.transport),
                    secrets,
                    &mut rng,
                )?;
                Ok(SealedSecrets {
                    index,
                    encapsulated_key: envelope.encapsulated_key,
                    ciphertext: envelope.ciphertext,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let plan = LobbyPlan {
            lobby,
            protocol_version: plan_version(&record.members)?,
            session,
            threshold,
            viewing_threshold,
            salt,
            members: record.members.clone(),
            secrets: sealed,
        };
        let (frame, _) = self.sign(scope(&lobby), 0, Message::LobbyPlan(plan.clone()))?;
        let mut record = record.clone();
        record.status = LobbyStatus::Started;
        record.plan = Some(plan);
        self.persist_lobby(&record)?;
        self.0.secrets.destroy(&secret_name(&lobby))?;
        self.linger_lobby(&mut state, &record);
        state.lobbies.insert(lobby, record.clone());
        self.refresh_allowlist(&state);
        drop(state);
        self.send_lobby_frame(record.peers(&self.identity()), &frame);
        self.emit_lobby(&record, Some(LobbyChangeKind::Started), None);
        self.lobby_info(&record, true)
    }

    /// Closes an open lobby this keeper hosts (guests are told) or leaves one it joined (the host is told).
    ///
    /// # Errors
    /// [`Error::Invalid`] for an unknown lobby, the wrong role or a lobby that is not open.
    pub async fn close_lobby(&self, lobby: [u8; 16], role: LobbyRole) -> Result<LobbyInfo> {
        let mut state = self.0.state.lock().await;
        let record = state
            .lobbies
            .get(&lobby)
            .ok_or_else(|| Error::Invalid("unknown lobby".into()))?;
        if record.role != role {
            return Err(Error::Invalid(match role {
                LobbyRole::Host => "only the host can close a lobby; guests leave it".into(),
                LobbyRole::Guest => "the host closes its lobby instead of leaving it".into(),
            }));
        }
        if record.status != LobbyStatus::Open {
            return Err(Error::Invalid(format!(
                "lobby is already {}",
                status_label(record.status)
            )));
        }
        let peers = record.peers(&self.identity());
        let mut record = record.clone();
        record.status = LobbyStatus::Closed;
        let (frame, _) = self.sign(scope(&lobby), 0, Message::LobbyLeave(LobbyLeave { lobby }))?;
        self.persist_lobby(&record)?;
        self.0.secrets.destroy(&secret_name(&lobby))?;
        self.linger_lobby(&mut state, &record);
        state.lobbies.insert(lobby, record.clone());
        self.refresh_allowlist(&state);
        drop(state);
        self.send_lobby_frame(peers, &frame);
        self.emit_lobby(&record, Some(LobbyChangeKind::Closed), None);
        self.lobby_info(&record, false)
    }

    /// Host: removes `member` from an open lobby, tells it so, and rotates the invite secret so the invite it joined
    /// with no longer admits anyone; the remaining guests get the new roster and the host the new invite.
    ///
    /// # Errors
    /// [`Error::Invalid`] unless this keeper hosts the open lobby and `member` is a guest on it.
    pub async fn remove_from_lobby(
        &self,
        lobby: [u8; 16],
        member: IdentityPublic,
    ) -> Result<LobbyInfo> {
        let mut state = self.0.state.lock().await;
        let record = state
            .lobbies
            .get(&lobby)
            .filter(|r| r.role == LobbyRole::Host && r.status == LobbyStatus::Open)
            .ok_or_else(|| {
                Error::Invalid("only the host can remove someone from an open lobby".into())
            })?;
        if member == self.identity() {
            return Err(Error::Invalid("the host closes its lobby instead".into()));
        }
        let removed = record
            .members
            .iter()
            .find(|m| m.keeper.identity == member)
            .map(|m| participant(&m.keeper))
            .ok_or_else(|| Error::Invalid("no such member in the lobby".into()))?;
        let mut record = record.clone();
        record.members.retain(|m| m.keeper.identity != member);
        self.0
            .secrets
            .put(&secret_name(&lobby), &JoinSecret(random_bytes()).0)?;
        let (notice, _) = self.sign(
            scope(&lobby),
            0,
            Message::LobbyRemoved(LobbyRemoved { lobby }),
        )?;
        // It may still be retrying its join: keep its connection a little longer.
        let until = self.now() + self.0.config.round_timeout_secs;
        let entry = state
            .lingering
            .entry(scope(&lobby))
            .or_insert((until, Vec::new()));
        entry.0 = until;
        entry.1.push(removed.clone());
        let (peers, frame) =
            self.roster_changed(&mut state, record, LobbyChangeKind::Removed, Some(member))?;
        let info = self.lobby_info(state.lobbies.get(&lobby).ok_or(Error::UnknownVault)?, false)?;
        drop(state);
        self.send_lobby_frame(vec![removed], &notice);
        self.send_lobby_frame(peers, &frame);
        Ok(info)
    }

    /// Host: changes an open lobby's size or thresholds; the guests get the new roster and the host the new invite.
    ///
    /// # Errors
    /// [`Error::Invalid`] unless this keeper hosts the open lobby and the new plan fits (a size from 2 to 8 and no
    /// smaller than the roster, thresholds within it).
    pub async fn update_lobby(
        &self,
        lobby: [u8; 16],
        members: Option<u8>,
        threshold: Option<u8>,
        viewing_threshold: Option<u8>,
    ) -> Result<LobbyInfo> {
        let mut state = self.0.state.lock().await;
        let record = state
            .lobbies
            .get(&lobby)
            .filter(|r| r.role == LobbyRole::Host && r.status == LobbyStatus::Open)
            .ok_or_else(|| Error::Invalid("only the host can change an open lobby".into()))?;
        let mut record = record.clone();
        if let Some(members) = members {
            if !(2..=MAX_MEMBERS).contains(&usize::from(members)) {
                return Err(Error::Invalid(format!(
                    "a vault has 2 to {MAX_MEMBERS} members"
                )));
            }
            if usize::from(members) < record.members.len() {
                return Err(Error::Invalid(format!(
                    "{} members have joined already; remove some first",
                    record.members.len()
                )));
            }
            record.expected = members;
        }
        record.threshold = threshold.unwrap_or(record.threshold);
        record.viewing_threshold = viewing_threshold.unwrap_or(record.viewing_threshold);
        check_thresholds(
            record.threshold,
            record.viewing_threshold,
            usize::from(record.expected.max(2)),
        )?;
        let (peers, frame) =
            self.roster_changed(&mut state, record, LobbyChangeKind::Updated, None)?;
        let info = self.lobby_info(state.lobbies.get(&lobby).ok_or(Error::UnknownVault)?, false)?;
        drop(state);
        self.send_lobby_frame(peers, &frame);
        Ok(info)
    }

    /// A lobby, with this member's opened plan secrets when `with_secrets`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for an unknown lobby; envelope errors if the stored plan no longer opens.
    pub async fn lobby(&self, lobby: [u8; 16], with_secrets: bool) -> Result<LobbyInfo> {
        let record = self
            .0
            .state
            .lock()
            .await
            .lobbies
            .get(&lobby)
            .cloned()
            .ok_or_else(|| Error::Invalid("unknown lobby".into()))?;
        self.lobby_info(&record, with_secrets)
    }

    /// Every lobby (without plan secrets).
    ///
    /// # Errors
    /// As [`Keeper::lobby`].
    pub async fn lobbies(&self) -> Result<Vec<LobbyInfo>> {
        let mut records: Vec<LobbyRecord> = self
            .0
            .state
            .lock()
            .await
            .lobbies
            .values()
            .cloned()
            .collect();
        records.sort_by_key(|r| r.lobby);
        records.iter().map(|r| self.lobby_info(r, false)).collect()
    }

    fn lobby_info(&self, record: &LobbyRecord, with_secrets: bool) -> Result<LobbyInfo> {
        let plan = record
            .plan
            .as_ref()
            .map(|plan| {
                Ok::<_, Error>(LobbyPlanInfo {
                    protocol_version: plan.protocol_version,
                    session: plan.session,
                    threshold: plan.threshold,
                    viewing_threshold: plan.viewing_threshold,
                    salt: plan.salt,
                    secrets: if with_secrets {
                        Some(self.open_plan(record, plan)?)
                    } else {
                        None
                    },
                })
            })
            .transpose()?;
        let invite = if record.role == LobbyRole::Host && record.status == LobbyStatus::Open {
            let secret = self.0.secrets.get(&secret_name(&record.lobby))?;
            let secret: [u8; 32] = secret
                .as_slice()
                .try_into()
                .map_err(|_| Error::Unseal(secret_name(&record.lobby)))?;
            Some(
                Invite {
                    lobby: record.lobby,
                    secret: JoinSecret(secret),
                    host: record.host.clone(),
                    host_name: record.host_name.clone(),
                    name: record.name.clone(),
                    threshold: record.threshold,
                    viewing_threshold: record.viewing_threshold,
                    members: record
                        .expected
                        .max(record.members.len().try_into().unwrap_or(u8::MAX)),
                    network: record.network.clone(),
                }
                .encode()?,
            )
        } else {
            None
        };
        Ok(LobbyInfo {
            lobby: record.lobby,
            role: record.role,
            status: record.status,
            name: record.name.clone(),
            host_name: record.host_name.clone(),
            threshold: record.threshold,
            viewing_threshold: record.viewing_threshold,
            network: record.network.clone(),
            host: record.host.clone(),
            members: record.members.clone(),
            plan,
            invite,
            expected: record.expected,
            removed: record.removed,
        })
    }

    /// Opens this member's sealed secrets in a plan (they stay sealed at rest under its transport key).
    fn open_plan(&self, record: &LobbyRecord, plan: &LobbyPlan) -> Result<Zeroizing<Vec<u8>>> {
        let index = index_of(&plan.members, &self.identity())
            .ok_or_else(|| Error::Invalid("this keeper is not in the plan".into()))?;
        let sealed = plan
            .sealed_for(index)
            .ok_or_else(|| Error::Invalid("the plan has no secrets for this member".into()))?;
        Ok(open_secrets(
            &plan.session,
            u32::from(index),
            &self.0.keys.transport,
            &TransportPublic(record.host.transport),
            &Envelope {
                encapsulated_key: sealed.encapsulated_key,
                ciphertext: sealed.ciphertext.clone(),
            },
        )?)
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Inbound

    /// A lobby frame (signature already verified): binds the signer to the channel, applies the replay rules and
    /// hands the body to the host or guest side.
    pub(crate) async fn on_lobby_frame(&self, message: SignedMessage, channel: Channel) {
        let signer = message.signer;
        let Some(lobby) = lobby_of(&message.body) else {
            return;
        };
        if message.header.scope != scope(&lobby) {
            return;
        }
        let mut state = self.0.state.lock().await;
        // The transport key the signer must have used: the one it claims when joining, otherwise its known key.
        let expected = match &message.body {
            Message::LobbyJoin(join) => {
                (join.member.keeper.identity == signer).then_some(join.member.keeper.transport)
            }
            _ => state.lobbies.get(&lobby).and_then(|record| {
                if record.host.identity == signer {
                    Some(record.host.transport)
                } else {
                    record
                        .members
                        .iter()
                        .find(|m| m.keeper.identity == signer)
                        .map(|m| m.keeper.transport)
                }
            }),
        };
        let bound = match (expected, channel) {
            (Some(key), Channel::Key(channel_key)) => key == channel_key,
            (Some(_), Channel::Peer(peer)) => peer == signer,
            (None, _) => false,
        };
        if !bound {
            tracing::debug!("dropped a lobby frame: signer does not match the channel");
            return;
        }
        let now = self.now();
        if let Err(error) = state.replay.check(&signer, &message.header, 0, now) {
            tracing::debug!(%error, "dropped a lobby frame");
            return;
        }
        let result = match message.body {
            Message::LobbyJoin(join) => self.on_lobby_join(&mut state, signer, join),
            Message::LobbyState(roster) => self.on_lobby_state(&mut state, signer, roster),
            Message::LobbyPlan(plan) => self.on_lobby_plan(&mut state, signer, plan),
            Message::LobbyLeave(leave) => self.on_lobby_leave(&mut state, signer, leave),
            Message::LobbyRemoved(removed) => self.on_lobby_removed(&mut state, signer, removed),
            _ => Ok(None),
        };
        drop(state);
        match result {
            Ok(Some((peers, frame))) => self.send_lobby_frame(peers, &frame),
            Ok(None) => {}
            Err(error) => tracing::debug!(%error, "refused a lobby message"),
        }
    }

    /// Host: admits or re-admits a joiner and returns the roster to broadcast.
    fn on_lobby_join(
        &self,
        state: &mut State,
        signer: IdentityPublic,
        join: LobbyJoin,
    ) -> Result<Option<(Vec<Participant>, Vec<u8>)>> {
        let lobby = join.lobby;
        let record = state
            .lobbies
            .get(&lobby)
            .filter(|r| r.role == LobbyRole::Host && r.status == LobbyStatus::Open)
            .ok_or_else(|| Error::Invalid("not an open lobby of this keeper".into()))?;
        let secret = self.0.secrets.get(&secret_name(&lobby))?;
        let secret: [u8; 32] = secret
            .as_slice()
            .try_into()
            .map_err(|_| Error::Unseal(secret_name(&lobby)))?;
        verify_join(&join, &JoinSecret(secret), &signer)?;
        let member = join.member;
        if signer == self.identity() {
            return Err(Error::Invalid("joiner is this keeper".into()));
        }
        check_point(&member.signer_key)?;
        let others = record
            .members
            .iter()
            .filter(|m| m.keeper.identity != signer);
        let mut count = 1;
        for other in others {
            count += 1;
            if other.signer_key == member.signer_key {
                return Err(Error::Invalid("signing key already on the roster".into()));
            }
            if other.keeper.transport == member.keeper.transport {
                return Err(Error::Invalid("transport key already on the roster".into()));
            }
        }
        let limit = if record.expected > 0 {
            usize::from(record.expected)
        } else {
            MAX_MEMBERS
        };
        if count > limit {
            return Err(Error::Full { members: limit });
        }
        let mut record = record.clone();
        match record
            .members
            .iter_mut()
            .find(|m| m.keeper.identity == signer)
        {
            Some(existing) => *existing = member,
            None => record.members.push(member),
        }
        self.roster_changed(state, record, LobbyChangeKind::Joined, Some(signer))
            .map(Some)
    }

    /// Host: persists a changed roster, re-admits peers and signs the state for every guest.
    fn roster_changed(
        &self,
        state: &mut State,
        mut record: LobbyRecord,
        kind: LobbyChangeKind,
        member: Option<IdentityPublic>,
    ) -> Result<(Vec<Participant>, Vec<u8>)> {
        record.revision += 1;
        let (frame, _) = self.sign(
            scope(&record.lobby),
            0,
            Message::LobbyState(LobbyState {
                lobby: record.lobby,
                revision: record.revision,
                name: record.name.clone(),
                threshold: record.threshold,
                viewing_threshold: record.viewing_threshold,
                expected: record.expected,
                members: record.members.clone(),
                started: record.status == LobbyStatus::Started,
            }),
        )?;
        self.persist_lobby(&record)?;
        let peers = record.peers(&self.identity());
        self.emit_lobby(&record, Some(kind), member);
        state.lobbies.insert(record.lobby, record);
        self.refresh_allowlist(state);
        Ok((peers, frame))
    }

    /// Guest: applies a newer roster from the host.
    fn on_lobby_state(
        &self,
        state: &mut State,
        signer: IdentityPublic,
        roster: LobbyState,
    ) -> Result<Option<(Vec<Participant>, Vec<u8>)>> {
        let record = state
            .lobbies
            .get(&roster.lobby)
            .filter(|r| r.role == LobbyRole::Guest && r.status == LobbyStatus::Open)
            .ok_or_else(|| Error::Invalid("not an open lobby this keeper joined".into()))?;
        if signer != record.host.identity
            || roster.members.first().map(|m| m.keeper.identity) != Some(signer)
        {
            return Err(Error::Invalid("roster not from the host".into()));
        }
        if roster.revision <= record.revision {
            return Ok(None);
        }
        let mut record = record.clone();
        let joined: Vec<IdentityPublic> = roster
            .members
            .iter()
            .map(|m| m.keeper.identity)
            .filter(|id| !record.members.iter().any(|m| &m.keeper.identity == id))
            .collect();
        let left = record
            .members
            .iter()
            .map(|m| m.keeper.identity)
            .find(|id| !roster.members.iter().any(|m| &m.keeper.identity == id));
        let kind = if joined.is_empty() && left.is_none() {
            (LobbyChangeKind::Updated, None)
        } else if let Some(id) = joined.first() {
            (LobbyChangeKind::Joined, Some(*id))
        } else {
            (LobbyChangeKind::Left, left)
        };
        record.revision = roster.revision;
        record.name = roster.name;
        record.threshold = roster.threshold;
        record.viewing_threshold = roster.viewing_threshold;
        record.expected = roster.expected;
        record.members = roster.members;
        self.persist_lobby(&record)?;
        self.emit_lobby(&record, Some(kind.0), kind.1);
        state.lobbies.insert(record.lobby, record);
        Ok(None)
    }

    /// Guest: accepts the host's plan if it names this member as it joined and its secrets open.
    fn on_lobby_plan(
        &self,
        state: &mut State,
        signer: IdentityPublic,
        plan: LobbyPlan,
    ) -> Result<Option<(Vec<Participant>, Vec<u8>)>> {
        let record = state
            .lobbies
            .get(&plan.lobby)
            .filter(|r| r.role == LobbyRole::Guest && r.status == LobbyStatus::Open)
            .ok_or_else(|| Error::Invalid("not an open lobby this keeper joined".into()))?;
        if signer != record.host.identity
            || plan.members.first().map(|m| &m.keeper) != Some(&record.host)
        {
            return Err(Error::Invalid("plan not from the host".into()));
        }
        check_thresholds(plan.threshold, plan.viewing_threshold, plan.members.len())?;
        for (position, member) in plan.members.iter().enumerate() {
            check_point(&member.signer_key)?;
            if plan.members[..position].iter().any(|m| {
                m.keeper.identity == member.keeper.identity || m.signer_key == member.signer_key
            }) {
                return Err(Error::Invalid("duplicate member in the plan".into()));
            }
        }
        let me = self.identity();
        let mine = plan
            .members
            .iter()
            .find(|m| m.keeper.identity == me)
            .ok_or_else(|| Error::Invalid("the plan does not include this keeper".into()))?;
        if mine.signer_key != record.me.signer_key
            || mine.keeper.transport != record.me.keeper.transport
        {
            return Err(Error::Invalid("the plan misstates this member".into()));
        }
        let mut record = record.clone();
        self.open_plan(&record, &plan)?;
        record.status = LobbyStatus::Started;
        record.members.clone_from(&plan.members);
        record.threshold = plan.threshold;
        record.viewing_threshold = plan.viewing_threshold;
        record.plan = Some(plan);
        self.persist_lobby(&record)?;
        self.0.secrets.destroy(&secret_name(&record.lobby))?;
        self.linger_lobby(state, &record);
        self.emit_lobby(&record, Some(LobbyChangeKind::Started), None);
        state.lobbies.insert(record.lobby, record);
        self.refresh_allowlist(state);
        Ok(None)
    }

    /// Host: a guest left (the remaining guests get the new roster). Guest: the host closed the lobby.
    fn on_lobby_leave(
        &self,
        state: &mut State,
        signer: IdentityPublic,
        leave: LobbyLeave,
    ) -> Result<Option<(Vec<Participant>, Vec<u8>)>> {
        let record = state
            .lobbies
            .get(&leave.lobby)
            .filter(|r| r.status == LobbyStatus::Open)
            .ok_or_else(|| Error::Invalid("not an open lobby".into()))?
            .clone();
        match record.role {
            LobbyRole::Host => {
                if signer == self.identity()
                    || !record.members.iter().any(|m| m.keeper.identity == signer)
                {
                    return Ok(None);
                }
                let leaving = record
                    .members
                    .iter()
                    .find(|m| m.keeper.identity == signer)
                    .map(|m| participant(&m.keeper));
                let mut record = record;
                record.members.retain(|m| m.keeper.identity != signer);
                if let Some(leaving) = leaving {
                    // Keep its connection a little longer: it may still be retrying its leave.
                    let until = self.now() + self.0.config.round_timeout_secs;
                    let entry = state
                        .lingering
                        .entry(scope(&record.lobby))
                        .or_insert((until, Vec::new()));
                    entry.0 = until;
                    entry.1.push(leaving);
                }
                self.roster_changed(state, record, LobbyChangeKind::Left, Some(signer))
                    .map(Some)
            }
            LobbyRole::Guest => {
                if signer != record.host.identity {
                    return Ok(None);
                }
                let mut record = record;
                record.status = LobbyStatus::Closed;
                self.persist_lobby(&record)?;
                self.0.secrets.destroy(&secret_name(&record.lobby))?;
                self.emit_lobby(&record, Some(LobbyChangeKind::Closed), None);
                state.lobbies.insert(record.lobby, record);
                self.refresh_allowlist(state);
                Ok(None)
            }
        }
    }

    /// Guest: the host removed this member from the lobby.
    fn on_lobby_removed(
        &self,
        state: &mut State,
        signer: IdentityPublic,
        removed: LobbyRemoved,
    ) -> Result<Option<(Vec<Participant>, Vec<u8>)>> {
        let record = state
            .lobbies
            .get(&removed.lobby)
            .filter(|r| r.role == LobbyRole::Guest && r.status == LobbyStatus::Open)
            .ok_or_else(|| Error::Invalid("not an open lobby this keeper joined".into()))?;
        if signer != record.host.identity {
            return Ok(None);
        }
        let mut record = record.clone();
        record.status = LobbyStatus::Closed;
        record.removed = true;
        self.persist_lobby(&record)?;
        self.0.secrets.destroy(&secret_name(&record.lobby))?;
        self.emit_lobby(
            &record,
            Some(LobbyChangeKind::Removed),
            Some(self.identity()),
        );
        state.lobbies.insert(record.lobby, record);
        self.refresh_allowlist(state);
        Ok(None)
    }
}

/// Peers to admit for open lobbies, and whether this keeper hosts one (strangers may then connect).
pub(crate) fn lobby_admission(state: &State, me: &IdentityPublic) -> (Vec<Peer>, bool) {
    let mut peers = Vec::new();
    let mut hosting = false;
    for record in state
        .lobbies
        .values()
        .filter(|r| r.status == LobbyStatus::Open)
    {
        hosting |= record.role == LobbyRole::Host;
        peers.extend(record.peers(me).iter().map(Peer::from));
    }
    (peers, hosting)
}

/// Transport keys of open lobbies' peers (their relay mailboxes are polled too).
pub(crate) fn lobby_mailbox_senders(state: &State, me: &IdentityPublic) -> Vec<[u8; 32]> {
    lobby_admission(state, me)
        .0
        .into_iter()
        .map(|p| p.transport)
        .collect()
}
