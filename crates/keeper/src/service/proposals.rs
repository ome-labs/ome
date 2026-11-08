//! Spending records: proposals, their payloads, approvals, rejections and execution receipts, stored and replicated to
//! every member. A vote is verified against the vault's signer keys before it is stored; one that arrives before its
//! proposal is held until the proposal does. Payloads are fetched by hash: a keeper that receives a proposal whose
//! payload it lacks asks the sender for it.

use ome_api::keeper::KeeperEvent;
use ome_crypto::{Point, schnorr};
use ome_net::Peer;
use ome_protocol::vault::{approval_message, rejection_message};
use ome_wire::{
    Header, IdentityPublic, Message, decode_body, encode_body,
    messages::proposals::{
        Approval, ExecutionReceipt, PayloadRequest, Proposal, ProposalPayload, Rejection,
    },
    types::{WireSignature, parse_field},
};

use crate::{
    Error, Result,
    keeper::Keeper,
    store::{ManifestStatus, ReceiptEntry},
};

/// Votes held while their proposal has not arrived yet.
const MAX_ORPHANS: usize = 1024;

/// A replicated vote, held until its proposal arrives.
pub(crate) enum Vote {
    Approval(Approval),
    Rejection(Rejection),
}

impl Vote {
    fn proposal(&self) -> [u8; 32] {
        match self {
            Self::Approval(approval) => approval.proposal,
            Self::Rejection(rejection) => rejection.proposal,
        }
    }
}

impl Keeper {
    /// Stores a proposal for `vault` and pushes it to the other members. Returns its id.
    ///
    /// # Errors
    /// [`Error::UnknownVault`], decoding errors.
    pub async fn put_proposal(&self, vault: [u8; 32], body: &[u8]) -> Result<[u8; 32]> {
        let Message::Proposal(proposal) = decode_body(body)? else {
            return Err(Error::Invalid("not a proposal".into()));
        };
        let id = self.store_proposal(vault, &proposal)?;
        self.adopt_orphans(vault, id).await;
        self.broadcast(vault, Message::Proposal(proposal)).await?;
        Ok(id)
    }

    /// Stores a proposal payload from this keeper's member. Payloads are not pushed: members fetch them by hash from
    /// whoever sent them the proposal, so a payload never travels to a member that does not know why it exists.
    /// Returns its hash.
    ///
    /// # Errors
    /// [`Error::UnknownVault`], decoding or size errors.
    pub async fn put_payload(&self, vault: [u8; 32], body: &[u8]) -> Result<[u8; 32]> {
        let Message::Payload(payload) = decode_body(body)? else {
            return Err(Error::Invalid("not a payload".into()));
        };
        self.active_vault(vault).await?;
        self.store_payload(vault, &payload)
    }

    /// Verifies and stores an approval (the slot's signer over the proposal's approval message) and pushes it to the
    /// other members.
    ///
    /// # Errors
    /// [`Error::Invalid`] for an unknown proposal or an invalid signature.
    pub async fn put_approval(&self, vault: [u8; 32], body: &[u8]) -> Result<()> {
        let Message::Approval(approval) = decode_body(body)? else {
            return Err(Error::Invalid("not an approval".into()));
        };
        self.store_approval(vault, &approval).await?;
        self.broadcast(vault, Message::Approval(approval)).await
    }

    /// Verifies and stores a rejection (the slot's signer over the proposal's rejection message) and pushes it to the
    /// other members.
    ///
    /// # Errors
    /// [`Error::Invalid`] for an unknown proposal or an invalid signature.
    pub async fn put_rejection(&self, vault: [u8; 32], body: &[u8]) -> Result<()> {
        let Message::Rejection(rejection) = decode_body(body)? else {
            return Err(Error::Invalid("not a rejection".into()));
        };
        self.store_rejection(vault, &rejection).await?;
        self.broadcast(vault, Message::Rejection(rejection)).await
    }

    /// Records that `proposal` executed in `tx_hash` and tells the other members.
    ///
    /// # Errors
    /// Storage errors.
    pub async fn put_receipt(
        &self,
        vault: [u8; 32],
        proposal: [u8; 32],
        tx_hash: [u8; 32],
    ) -> Result<()> {
        self.store_receipt(vault, proposal, tx_hash)?;
        self.broadcast(
            vault,
            Message::ExecutionReceipt(ExecutionReceipt { proposal, tx_hash }),
        )
        .await
    }

    /// Checks a vault transaction the daemon observed against known executions; an unknown one raises
    /// [`KeeperEvent::UnexpectedSpend`]. Returns whether it was known.
    ///
    /// # Errors
    /// Storage errors.
    pub fn observe_tx(&self, vault: [u8; 32], tx_hash: [u8; 32]) -> Result<bool> {
        let known = self.0.store.has_receipt(&vault, &tx_hash)?;
        if !known {
            self.emit(KeeperEvent::UnexpectedSpend {
                vault: vault.into(),
                tx_hash: tx_hash.into(),
            });
        }
        Ok(known)
    }

    /// Proposal bodies of `vault`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn proposals(&self, vault: [u8; 32]) -> Result<Vec<Vec<u8>>> {
        self.0.store.proposals(&vault)
    }

    /// A proposal body.
    ///
    /// # Errors
    /// Storage errors.
    pub fn proposal(&self, id: [u8; 32]) -> Result<Option<Vec<u8>>> {
        Ok(self.0.store.proposal(&id)?.map(|(_, body)| body))
    }

    /// A payload body (a `Message::Payload`), by hash.
    ///
    /// # Errors
    /// Storage errors.
    pub fn payload(&self, vault: [u8; 32], hash: [u8; 32]) -> Result<Option<Vec<u8>>> {
        self.0
            .store
            .payload(&vault, &hash)?
            .map(|bytes| Ok(encode_body(&Message::Payload(ProposalPayload { bytes }))?))
            .transpose()
    }

    /// Receipts of `vault`, newest first.
    ///
    /// # Errors
    /// Storage errors.
    pub fn receipts(&self, vault: [u8; 32]) -> Result<Vec<ReceiptEntry>> {
        self.0.store.receipt_entries(&vault)
    }

    /// Approval bodies of a proposal.
    ///
    /// # Errors
    /// Storage errors.
    pub fn approvals(&self, proposal: [u8; 32]) -> Result<Vec<Vec<u8>>> {
        self.0.store.approvals(&proposal)
    }

    /// Rejection bodies of a proposal.
    ///
    /// # Errors
    /// Storage errors.
    pub fn rejections(&self, proposal: [u8; 32]) -> Result<Vec<Vec<u8>>> {
        self.0.store.rejections(&proposal)
    }

    pub(crate) async fn on_replicated(&self, from: IdentityPublic, header: Header, body: Message) {
        let vault = header.scope;
        let Some(sender) = self.member_peer(vault, &from).await else {
            return;
        };
        let result = match body {
            Message::Proposal(proposal) => match self.store_proposal(vault, &proposal) {
                Ok(id) => {
                    self.adopt_orphans(vault, id).await;
                    self.fetch_payload(vault, header.epoch, &proposal, sender)
                        .await;
                    Ok(())
                }
                Err(error) => Err(error),
            },
            Message::Payload(payload) => self.store_referenced_payload(vault, &payload).map(drop),
            Message::Approval(approval) => {
                match self.hold_orphan(vault, Vote::Approval(approval)).await {
                    Some(Vote::Approval(approval)) => self.store_approval(vault, &approval).await,
                    _ => return,
                }
            }
            Message::Rejection(rejection) => match self
                .hold_orphan(vault, Vote::Rejection(rejection))
                .await
            {
                Some(Vote::Rejection(rejection)) => self.store_rejection(vault, &rejection).await,
                _ => return,
            },
            Message::ExecutionReceipt(receipt) => {
                self.store_receipt(vault, receipt.proposal, receipt.tx_hash)
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            tracing::debug!(%error, "dropped a replicated record");
        }
    }

    /// Answers a member's request for payloads with each one this keeper holds.
    pub(crate) async fn on_payload_request(
        &self,
        from: IdentityPublic,
        header: Header,
        request: PayloadRequest,
    ) {
        let vault = header.scope;
        let Some(peer) = self.member_peer(vault, &from).await else {
            return;
        };
        for hash in request.hashes {
            let Ok(Some(bytes)) = self.0.store.payload(&vault, &hash) else {
                continue;
            };
            let Ok((frame, _)) = self.sign(
                vault,
                header.epoch,
                Message::Payload(ProposalPayload { bytes }),
            ) else {
                continue;
            };
            self.send(peer.clone(), frame).await;
        }
    }

    /// Asks `sender` for a proposal's payload if this keeper does not hold it yet.
    async fn fetch_payload(&self, vault: [u8; 32], epoch: u32, proposal: &Proposal, sender: Peer) {
        if matches!(
            self.0.store.payload(&vault, &proposal.payload.hash),
            Ok(Some(_))
        ) {
            return;
        }
        let request = Message::PayloadRequest(PayloadRequest {
            hashes: vec![proposal.payload.hash],
        });
        if let Ok((frame, _)) = self.sign(vault, epoch, request) {
            self.send(sender, frame).await;
        }
    }

    /// The transport peer of `identity` if it is a member of `vault`'s active committee.
    async fn member_peer(&self, vault: [u8; 32], identity: &IdentityPublic) -> Option<Peer> {
        let state = self.0.state.lock().await;
        let v = state
            .vaults
            .get(&vault)
            .filter(|v| v.status == ManifestStatus::Active)?;
        v.committee.member(identity).map(Peer::from)
    }

    async fn active_vault(&self, vault: [u8; 32]) -> Result<()> {
        let state = self.0.state.lock().await;
        state
            .vaults
            .get(&vault)
            .filter(|v| v.status == ManifestStatus::Active)
            .map(drop)
            .ok_or(Error::UnknownVault)
    }

    /// Keeps a vote whose proposal has not arrived yet (records are processed concurrently) and returns `None`, or
    /// hands the vote back when its proposal is stored. The check runs under the state lock that
    /// [`Self::adopt_orphans`] takes after storing a proposal, so a vote is never parked after its proposal's adoption.
    async fn hold_orphan(&self, vault: [u8; 32], vote: Vote) -> Option<Vote> {
        let proposal = vote.proposal();
        let mut state = self.0.state.lock().await;
        if self.0.store.proposal(&proposal).ok().flatten().is_some() {
            return Some(vote);
        }
        if state.orphans.values().map(Vec::len).sum::<usize>() < MAX_ORPHANS {
            state
                .orphans
                .entry(proposal)
                .or_default()
                .push((vault, vote));
        }
        None
    }

    /// Stores votes that arrived before proposal `id`.
    async fn adopt_orphans(&self, vault: [u8; 32], id: [u8; 32]) {
        let waiting = self
            .0
            .state
            .lock()
            .await
            .orphans
            .remove(&id)
            .unwrap_or_default();
        for (orphan_vault, vote) in waiting {
            if orphan_vault != vault {
                continue;
            }
            let result = match vote {
                Vote::Approval(approval) => self.store_approval(vault, &approval).await,
                Vote::Rejection(rejection) => self.store_rejection(vault, &rejection).await,
            };
            if let Err(error) = result {
                tracing::debug!(%error, "dropped an early vote");
            }
        }
    }

    fn store_proposal(&self, vault: [u8; 32], proposal: &Proposal) -> Result<[u8; 32]> {
        let id = proposal.id()?;
        let body = encode_body(&Message::Proposal(proposal.clone()))?;
        if self.0.store.put_proposal(&id, &vault, &body, self.now())? {
            self.emit(KeeperEvent::Proposal {
                vault: vault.into(),
                id: id.into(),
            });
        }
        Ok(id)
    }

    fn store_payload(&self, vault: [u8; 32], payload: &ProposalPayload) -> Result<[u8; 32]> {
        let hash = payload.reference()?.hash;
        if self
            .0
            .store
            .put_payload(&hash, &vault, &payload.bytes, self.now())?
        {
            self.emit(KeeperEvent::Payload {
                vault: vault.into(),
                hash: hash.into(),
            });
        }
        Ok(hash)
    }

    /// Stores a replicated payload only if a stored proposal of `vault` references it, so a member cannot fill
    /// another member's store with unrelated bytes.
    fn store_referenced_payload(
        &self,
        vault: [u8; 32],
        payload: &ProposalPayload,
    ) -> Result<[u8; 32]> {
        let referenced = self.0.store.proposals(&vault)?.iter().any(|body| {
            matches!(decode_body(body), Ok(Message::Proposal(proposal)) if proposal.payload.matches(payload))
        });
        if !referenced {
            return Err(Error::Invalid("payload of no known proposal".into()));
        }
        self.store_payload(vault, payload)
    }

    fn store_receipt(&self, vault: [u8; 32], proposal: [u8; 32], tx_hash: [u8; 32]) -> Result<()> {
        if self
            .0
            .store
            .put_receipt(&proposal, &vault, &tx_hash, self.now())?
        {
            self.emit(KeeperEvent::Receipt {
                vault: vault.into(),
                proposal: proposal.into(),
                tx_hash: tx_hash.into(),
            });
        }
        Ok(())
    }

    /// The stored proposal `id` of `vault`.
    pub(crate) fn stored_proposal(&self, vault: [u8; 32], id: &[u8; 32]) -> Result<Proposal> {
        let (stored_vault, body) = self
            .0
            .store
            .proposal(id)?
            .ok_or_else(|| Error::Invalid("vote for an unknown proposal".into()))?;
        if stored_vault != vault {
            return Err(Error::Invalid("vote for another vault's proposal".into()));
        }
        let Message::Proposal(proposal) = decode_body(&body)? else {
            return Err(Error::Storage("stored proposal is not a proposal".into()));
        };
        Ok(proposal)
    }

    /// Checks that `signature` is the signer of `slot` in `vault`'s committee over `message`.
    async fn verify_vote(
        &self,
        vault: [u8; 32],
        slot: u32,
        message: ome_crypto::Base,
        signature: &WireSignature,
    ) -> Result<()> {
        let key = {
            let state = self.0.state.lock().await;
            let committee = &state
                .vaults
                .get(&vault)
                .ok_or(Error::UnknownVault)?
                .committee;
            let member = usize::try_from(slot)
                .ok()
                .and_then(|slot| committee.manifest.members.get(slot))
                .ok_or_else(|| Error::Invalid("vote for an empty slot".into()))?;
            Point::try_from(&member.signer_key)?
        };
        let signature = schnorr::Signature::try_from(signature)?;
        if schnorr::verify(&key, message, &signature) {
            Ok(())
        } else {
            Err(Error::Invalid("invalid vote signature".into()))
        }
    }

    pub(crate) async fn store_approval(&self, vault: [u8; 32], approval: &Approval) -> Result<()> {
        let proposal = self.stored_proposal(vault, &approval.proposal)?;
        let message = approval_message(
            parse_field(&vault)?,
            parse_field(&proposal.outer_hash)?,
            proposal.config_version,
            proposal.expires_at,
        );
        self.verify_vote(vault, approval.slot, message, &approval.signature)
            .await?;
        let body = encode_body(&Message::Approval(*approval))?;
        if self
            .0
            .store
            .put_approval(&approval.proposal, approval.slot, &body, self.now())?
        {
            self.emit(KeeperEvent::Approval {
                vault: vault.into(),
                proposal: approval.proposal.into(),
                slot: approval.slot,
            });
        }
        Ok(())
    }

    pub(crate) async fn store_rejection(
        &self,
        vault: [u8; 32],
        rejection: &Rejection,
    ) -> Result<()> {
        self.stored_proposal(vault, &rejection.proposal)?;
        let message = rejection_message(parse_field(&vault)?, &rejection.proposal);
        self.verify_vote(vault, rejection.slot, message, &rejection.signature)
            .await?;
        let body = encode_body(&Message::Rejection(rejection.clone()))?;
        if self
            .0
            .store
            .put_rejection(&rejection.proposal, rejection.slot, &body, self.now())?
        {
            self.emit(KeeperEvent::Rejection {
                vault: vault.into(),
                proposal: rejection.proposal.into(),
                slot: rejection.slot,
            });
        }
        Ok(())
    }

    /// Signs `body` for `vault` and sends it to every other member; one that cannot be reached gets it through its
    /// relay mailbox, and the next sync.
    pub(crate) async fn broadcast(&self, vault: [u8; 32], body: Message) -> Result<()> {
        let (peers, epoch) = {
            let state = self.0.state.lock().await;
            let v = state.vaults.get(&vault).ok_or(Error::UnknownVault)?;
            (
                v.committee.peers_except(&self.identity()),
                v.committee.epoch(),
            )
        };
        let (frame, _) = self.sign(vault, epoch, body)?;
        for peer in peers {
            let keeper = self.clone();
            let frame = frame.clone();
            tokio::spawn(async move {
                if !keeper.send(peer.clone(), frame.clone()).await
                    && let Err(error) = keeper.deposit(&peer, &frame).await
                {
                    tracing::debug!(%error, "mailbox deposit failed");
                }
            });
        }
        Ok(())
    }
}
