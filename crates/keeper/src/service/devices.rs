//! The member's paired signing devices. The keeper hands out one-time pairing offers, polls each open offer's pairing
//! mailbox for the device's request, and then talks to the device through their pairwise relay mailbox: it sends the
//! proposals the daemon asks it to, and stores and replicates the device's votes exactly like the daemon's. A vote
//! counts only for the slot whose key the device holds. See `architecture/mobile.md`.

use std::collections::HashMap;

use ome_api::keeper::KeeperEvent;
use ome_crypto::{
    Point,
    sealing::pairing::{self, pairing_mailbox},
};
use ome_net::{Channel, Peer};
use ome_protocol::{membership::swapped_manifest, payload::ExecutionRequest, vault::Config};
use ome_wire::{
    Header, IdentityPublic, Message, SignedMessage, decode,
    limits::MAX_REASON_BYTES,
    messages::{
        device::{
            Ballot, DeviceChain, DeviceConfig, DeviceLabel, DevicePair, DevicePaired, DeviceReview,
            DeviceRotation, DeviceUnpair, DeviceVote, DeviceVoted, MAX_FEE_PAYERS, PairOffer,
            PairSecret, pairing_scope,
        },
        proposals::ProposalPayload,
    },
    types::{WireField, field},
};
use zeroize::Zeroizing;

use super::vaults::VaultInfo;
use crate::{
    Error, Result,
    committee::Committee,
    keeper::{Keeper, State},
    random::random_bytes,
    store::{DeviceRecord, ManifestStatus},
};

/// How long a pairing offer stays open.
const OFFER_TTL_SECS: u64 = 600;
/// Most offers open at once.
const MAX_OFFERS: usize = 8;

/// A pairing offer waiting for its device. The secret lives only in memory: a keeper that restarts drops its offers.
pub(crate) struct OpenOffer {
    secret: Zeroizing<[u8; 32]>,
    expires_at: u64,
    member: String,
    network: String,
    fee_payers: Vec<WireField>,
}

/// A proposal to send to the member's device, with what only the daemon knows.
#[derive(Clone, Debug)]
pub struct DeviceReviewRequest {
    /// Vault.
    pub vault: [u8; 32],
    /// Proposal id.
    pub proposal: [u8; 32],
    /// The vault's display name.
    pub vault_name: String,
    /// The config note the daemon read.
    pub config: Config,
    /// The vault's chain.
    pub chain: DeviceChain,
    /// Names for the addresses involved.
    pub labels: Vec<DeviceLabel>,
    /// The daemon's simulation outcome.
    pub simulation: Option<String>,
    /// For a reshare rotation, the rotation this keeper reshared.
    pub rotation: Option<DeviceRotation>,
}

impl Keeper {
    pub(crate) fn load_devices(&self) -> Result<()> {
        let devices = self.0.store.devices()?;
        let mut state = self
            .0
            .state
            .try_lock()
            .map_err(|_| Error::Storage("state busy at start".into()))?;
        state
            .devices
            .extend(devices.into_iter().map(|d| (d.identity, d)));
        Ok(())
    }

    /// Opens a one-time pairing offer and returns its string form and expiry. `member`, `network` and `fee_payers` are
    /// what the device learns about its member; the fee payers are the ones a rotation may also call.
    ///
    /// # Errors
    /// [`Error::Invalid`] without a relay, with too many offers open or too many fee payers.
    pub async fn offer_device(
        &self,
        member: String,
        network: String,
        fee_payers: Vec<WireField>,
    ) -> Result<(String, u64)> {
        if self.0.mailbox.is_none() || self.0.relays.is_empty() {
            return Err(Error::Invalid("pairing a device needs a relay".into()));
        }
        if fee_payers.len() > MAX_FEE_PAYERS {
            return Err(Error::Invalid(format!(
                "at most {MAX_FEE_PAYERS} fee payers"
            )));
        }
        let now = self.now();
        let offer = random_bytes::<16>();
        let secret = Zeroizing::new(random_bytes::<32>());
        let expires_at = now + OFFER_TTL_SECS;
        let text = PairOffer {
            offer,
            keeper: self.identity(),
            transport: self.transport_key(),
            relays: self.0.relays.clone(),
            secret: PairSecret(*secret),
            expires_at,
            member: member.clone(),
            network: network.clone(),
        }
        .encode()?;
        let mut state = self.0.state.lock().await;
        state.offers.retain(|_, o| o.expires_at >= now);
        if state.offers.len() >= MAX_OFFERS {
            return Err(Error::Invalid("too many pairing offers open".into()));
        }
        state.offers.insert(
            offer,
            OpenOffer {
                secret,
                expires_at,
                member,
                network,
                fee_payers,
            },
        );
        Ok((text, expires_at))
    }

    /// Paired devices, oldest first.
    pub async fn devices(&self) -> Vec<DeviceRecord> {
        let state = self.0.state.lock().await;
        let mut devices: Vec<DeviceRecord> = state.devices.values().cloned().collect();
        devices.sort_by_key(|d| d.paired_at);
        devices
    }

    /// Unpairs a device and tells it. Its frames are ignored from now on.
    ///
    /// # Errors
    /// [`Error::Invalid`] for an unknown device, storage errors.
    pub async fn revoke_device(&self, identity: IdentityPublic) -> Result<()> {
        let device = self
            .0
            .state
            .lock()
            .await
            .devices
            .remove(&identity)
            .ok_or_else(|| Error::Invalid("no such device".into()))?;
        self.0.store.remove_device(&identity)?;
        let unpair = Message::DeviceUnpair(DeviceUnpair {
            reason: "the member unpaired this device".into(),
        });
        if let Err(error) = self.send_to_device(&device, [0; 32], 0, unpair).await {
            tracing::debug!(%error, "could not tell the device it was unpaired");
        }
        self.emit(KeeperEvent::DeviceUnpaired {
            identity: identity.0.into(),
        });
        Ok(())
    }

    /// Sends a stored proposal to every device that holds this member's signing key in the vault. Returns how many
    /// devices it was sent to.
    ///
    /// # Errors
    /// [`Error::UnknownVault`], [`Error::Invalid`] if this keeper is not a member, for an unknown proposal or a payload
    /// that has not arrived, mailbox errors.
    pub async fn review_on_devices(&self, request: DeviceReviewRequest) -> Result<u32> {
        let vault = request.vault;
        let (manifest, epoch, key) = {
            let state = self.0.state.lock().await;
            let v = state
                .vaults
                .get(&vault)
                .filter(|v| v.status == ManifestStatus::Active)
                .ok_or(Error::UnknownVault)?;
            let me = v
                .committee
                .member(&self.identity())
                .ok_or(Error::NotMember(self.identity()))?;
            (
                v.committee.manifest.clone(),
                v.committee.epoch(),
                me.signer_key,
            )
        };
        let proposal = self.stored_proposal(vault, &request.proposal)?;
        let bytes = self
            .0
            .store
            .payload(&vault, &proposal.payload.hash)?
            .ok_or_else(|| Error::Invalid("the proposal's payload has not arrived yet".into()))?;
        let devices: Vec<DeviceRecord> = {
            let state = self.0.state.lock().await;
            state
                .devices
                .values()
                .filter(|d| d.signer_key == key)
                .cloned()
                .collect()
        };
        let review = Message::DeviceReview(Box::new(DeviceReview {
            vault,
            vault_name: request.vault_name,
            manifest,
            config: device_config(&request.config),
            chain: request.chain,
            proposal,
            payload: ProposalPayload { bytes },
            labels: request.labels,
            simulation: request.simulation,
            rotation: request.rotation,
        }));
        for device in &devices {
            self.send_to_device(device, vault, epoch, review.clone())
                .await?;
        }
        Ok(u32::try_from(devices.len()).unwrap_or(u32::MAX))
    }

    /// Adopts the signer key an executed signer swap put on chain: the manifest the swap produces
    /// (`ome_protocol::membership::swapped_manifest`) must match the newest config the daemon observed.
    ///
    /// # Errors
    /// [`Error::UnknownVault`], [`Error::Invalid`] before the config was observed or without the payload, or the
    /// protocol error that refused the swap.
    pub async fn update_signers(&self, vault: [u8; 32], proposal: [u8; 32]) -> Result<VaultInfo> {
        let stored = self.stored_proposal(vault, &proposal)?;
        let bytes = self
            .0
            .store
            .payload(&vault, &stored.payload.hash)?
            .ok_or_else(|| Error::Invalid("the swap's payload is not stored".into()))?;
        let request = ExecutionRequest::from_payload(&ProposalPayload { bytes })?;
        let config = self
            .0
            .store
            .latest_observation(&vault)?
            .ok_or_else(|| Error::Invalid("observe the vault's config first".into()))?;
        let mut state = self.0.state.lock().await;
        let current = state
            .vaults
            .get_mut(&vault)
            .filter(|v| v.status == ManifestStatus::Active)
            .ok_or(Error::UnknownVault)?;
        let next = Committee::new(swapped_manifest(
            &current.committee.manifest,
            &config,
            &request.calls,
        )?)?;
        self.0
            .store
            .put_manifest(&next.manifest, ManifestStatus::Active, self.now())?;
        current.committee = next;
        self.refresh_allowlist(&state);
        Self::info(&state, &vault, &self.identity()).ok_or(Error::UnknownVault)
    }

    /// Senders whose mailboxes this keeper polls besides its committees: its paired devices.
    pub(crate) fn device_senders(state: &State) -> Vec<[u8; 32]> {
        state.devices.values().map(|d| d.transport).collect()
    }

    /// Collects pairing requests from every open offer's pairing mailbox. Returns how many devices paired.
    pub(crate) async fn poll_pairings(&self) -> Result<usize> {
        let Some(service) = self.0.mailbox.clone() else {
            return Ok(0);
        };
        let now = self.now();
        let offers: Vec<([u8; 16], Zeroizing<[u8; 32]>)> = {
            let mut state = self.0.state.lock().await;
            state.offers.retain(|_, o| o.expires_at >= now);
            state
                .offers
                .iter()
                .map(|(id, o)| (*id, o.secret.clone()))
                .collect()
        };
        let mut paired = 0;
        for (offer, secret) in offers {
            let envelopes = service.fetch(vec![pairing_mailbox(&secret)]).await?;
            let mut done = Vec::with_capacity(envelopes.len());
            for stored in envelopes {
                done.push(stored.id);
                let Ok(frame) = pairing::open(&secret, &stored.envelope.sealed) else {
                    continue;
                };
                let Ok(message) = decode(&frame) else {
                    continue;
                };
                if message.verify().is_err() || message.header.scope != pairing_scope(&offer) {
                    continue;
                }
                let Message::DevicePair(pair) = message.body else {
                    continue;
                };
                if pair.offer != offer {
                    continue;
                }
                match self.accept_pairing(message.signer, pair).await {
                    Ok(true) => paired += 1,
                    Ok(false) => {}
                    Err(error) => tracing::debug!(%error, "refused a pairing"),
                }
            }
            if !done.is_empty() {
                service.delete(done).await?;
            }
        }
        Ok(paired)
    }

    /// Pairs the device that answered `pair.offer`, consuming the offer, and tells it. Returns `false` if the offer
    /// was already used.
    async fn accept_pairing(&self, identity: IdentityPublic, pair: DevicePair) -> Result<bool> {
        Point::try_from(&pair.signer_key)?;
        let device = DeviceRecord {
            identity,
            transport: pair.transport,
            signer_key: pair.signer_key,
            name: pair.name,
            paired_at: self.now(),
        };
        let offer = {
            let mut state = self.0.state.lock().await;
            let Some(offer) = state.offers.remove(&pair.offer) else {
                return Ok(false);
            };
            if state
                .devices
                .values()
                .any(|d| d.transport == device.transport && d.identity != identity)
            {
                return Err(Error::Invalid(
                    "another device uses this transport key".into(),
                ));
            }
            self.0.store.put_device(&device)?;
            state.devices.insert(identity, device.clone());
            offer
        };
        let paired = Message::DevicePaired(DevicePaired {
            offer: pair.offer,
            member: offer.member,
            network: offer.network,
            fee_payers: offer.fee_payers,
        });
        self.send_to_device(&device, pairing_scope(&pair.offer), 0, paired)
            .await?;
        self.emit(KeeperEvent::DevicePaired {
            identity: identity.0.into(),
            name: device.name,
        });
        Ok(true)
    }

    /// Handles a frame from a paired device: it must arrive on that device's mailbox channel and be signed by it.
    pub(crate) async fn on_device_frame(&self, message: SignedMessage, channel: Channel) {
        let Channel::Key(key) = channel else {
            return;
        };
        let device = {
            let state = self.0.state.lock().await;
            match state.devices.get(&message.signer) {
                Some(device) if device.transport == key => device.clone(),
                _ => return,
            }
        };
        match message.body {
            Message::DeviceVote(vote) => self.on_device_vote(&device, message.header, vote).await,
            Message::DeviceUnpair(_) => {
                self.0.state.lock().await.devices.remove(&device.identity);
                if let Err(error) = self.0.store.remove_device(&device.identity) {
                    tracing::error!(%error, "cannot forget an unpaired device");
                }
                self.emit(KeeperEvent::DeviceUnpaired {
                    identity: device.identity.0.into(),
                });
            }
            _ => {}
        }
    }

    /// Stores and replicates a device's vote if the device holds the slot's key, and tells it the outcome.
    async fn on_device_vote(&self, device: &DeviceRecord, header: Header, vote: DeviceVote) {
        let vault = vote.vault;
        let proposal = vote.ballot.proposal();
        let approved = matches!(vote.ballot, Ballot::Approval(_));
        let outcome = self.store_device_vote(device, vote).await;
        let refused = outcome.err().map(|error| {
            let mut reason = error.to_string();
            reason.truncate(MAX_REASON_BYTES);
            reason
        });
        let answer = Message::DeviceVoted(DeviceVoted {
            proposal,
            refused: refused.clone(),
        });
        if let Err(error) = self
            .send_to_device(device, vault, header.epoch, answer)
            .await
        {
            tracing::debug!(%error, "could not answer a device's vote");
        }
        self.emit(KeeperEvent::DeviceVote {
            vault: vault.into(),
            proposal: proposal.into(),
            identity: device.identity.0.into(),
            approved,
            refused,
        });
    }

    async fn store_device_vote(&self, device: &DeviceRecord, vote: DeviceVote) -> Result<()> {
        let vault = vote.vault;
        let slot = match &vote.ballot {
            Ballot::Approval(approval) => approval.slot,
            Ballot::Rejection(rejection) => rejection.slot,
        };
        {
            let state = self.0.state.lock().await;
            let v = state
                .vaults
                .get(&vault)
                .filter(|v| v.status == ManifestStatus::Active)
                .ok_or(Error::UnknownVault)?;
            let holds = usize::try_from(slot)
                .ok()
                .and_then(|slot| v.committee.manifest.members.get(slot))
                .is_some_and(|m| m.signer_key == device.signer_key);
            if !holds {
                return Err(Error::Invalid(
                    "this device does not hold the slot's signing key".into(),
                ));
            }
        }
        match vote.ballot {
            Ballot::Approval(approval) => {
                self.store_approval(vault, &approval).await?;
                self.broadcast(vault, Message::Approval(approval)).await
            }
            Ballot::Rejection(rejection) => {
                self.store_rejection(vault, &rejection).await?;
                self.broadcast(vault, Message::Rejection(rejection)).await
            }
        }
    }

    /// Signs `body` and leaves it in the device's mailbox.
    async fn send_to_device(
        &self,
        device: &DeviceRecord,
        scope: [u8; 32],
        epoch: u32,
        body: Message,
    ) -> Result<()> {
        if self.0.mailbox.is_none() {
            return Err(Error::Invalid("reaching a device needs a relay".into()));
        }
        let (frame, _) = self.sign(scope, epoch, body)?;
        let peer = Peer {
            identity: device.identity,
            transport: device.transport,
            addresses: Vec::new(),
        };
        self.deposit(&peer, &frame).await
    }
}

/// The open offers, by id.
pub(crate) type Offers = HashMap<[u8; 16], OpenOffer>;

fn device_config(config: &Config) -> DeviceConfig {
    DeviceConfig {
        threshold: config.threshold,
        signer_count: config.signer_count,
        version: config.version,
        signer_set_commitment: field(&config.signer_set_commitment),
        viewing_threshold: config.viewing_threshold,
        committee_commitment: field(&config.committee_commitment),
    }
}

/// Messages only a paired device and its keeper exchange.
pub(crate) fn is_device_message(body: &Message) -> bool {
    matches!(
        body,
        Message::DevicePair(_)
            | Message::DevicePaired(_)
            | Message::DeviceReview(_)
            | Message::DeviceVote(_)
            | Message::DeviceVoted(_)
            | Message::DeviceUnpair(_)
    )
}
