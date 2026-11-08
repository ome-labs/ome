use std::collections::HashSet;

use ome_wire::{
    Header, IdentityPublic, Message, decode_body,
    limits::MAX_SYNC_ITEMS,
    messages::{
        proposals::ExecutionReceipt,
        sync::{SyncRequest, VoteRef},
    },
};

use crate::keeper::Keeper;
use ome_net::Peer;

use crate::{Error, Result, store::ManifestStatus};

/// The newest [`MAX_SYNC_ITEMS`] of `items` (oldest first), so a sync request stays within bounds.
fn newest<T>(mut items: Vec<T>) -> Vec<T> {
    let skip = items.len().saturating_sub(MAX_SYNC_ITEMS);
    items.drain(..skip);
    items
}

/// Most records sent back for one sync request (the requester syncs again for the rest).
const MAX_SYNC_RECORDS: usize = 1024;

impl Keeper {
    /// Tells every other member what this keeper holds for `vault`; they answer with what it lacks.
    ///
    /// # Errors
    /// [`Error::UnknownVault`] or storage errors.
    pub async fn sync(&self, vault: [u8; 32]) -> Result<()> {
        let (peers, epoch) = {
            let mut state = self.0.state.lock().await;
            let v = state
                .vaults
                .get(&vault)
                .filter(|v| v.status == ManifestStatus::Active)
                .ok_or(Error::UnknownVault)?;
            let found = (
                v.committee.peers_except(&self.identity()),
                v.committee.epoch(),
            );
            state.last_sync.insert(vault, self.now());
            found
        };
        let store = &self.0.store;
        let request = SyncRequest {
            proposals: newest(store.proposal_ids(&vault)?),
            approvals: newest(
                store
                    .approval_refs(&vault)?
                    .into_iter()
                    .map(|(proposal, slot)| VoteRef { proposal, slot })
                    .collect(),
            ),
            receipts: newest(
                store
                    .receipts(&vault)?
                    .into_iter()
                    .map(|(p, _)| p)
                    .collect(),
            ),
            rejections: newest(
                store
                    .rejection_refs(&vault)?
                    .into_iter()
                    .map(|(proposal, slot)| VoteRef { proposal, slot })
                    .collect(),
            ),
        };
        let (frame, _) = self.sign(vault, epoch, Message::SyncRequest(request))?;
        for peer in peers {
            let keeper = self.clone();
            let frame = frame.clone();
            tokio::spawn(async move {
                keeper.send(peer, frame).await;
            });
        }
        Ok(())
    }

    /// Answers a member's sync request with the proposals, approvals, rejections and receipts it lacks (proposals
    /// first).
    pub(crate) async fn on_sync_request(
        &self,
        from: IdentityPublic,
        header: Header,
        request: SyncRequest,
    ) {
        let vault = header.scope;
        let (peer, epoch): (Peer, u32) = {
            let state = self.0.state.lock().await;
            let Some(v) = state
                .vaults
                .get(&vault)
                .filter(|v| v.status == ManifestStatus::Active)
            else {
                return;
            };
            let Some(member) = v.committee.member(&from) else {
                return;
            };
            (Peer::from(member), v.committee.epoch())
        };
        let records = match self.missing(&vault, &request) {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(%error, "cannot answer a sync request");
                return;
            }
        };
        let keeper = self.clone();
        tokio::spawn(async move {
            for body in records {
                let Ok((frame, _)) = keeper.sign(vault, epoch, body) else {
                    continue;
                };
                if !keeper.send(peer.clone(), frame).await {
                    break;
                }
            }
        });
    }

    fn missing(&self, vault: &[u8; 32], request: &SyncRequest) -> Result<Vec<Message>> {
        let store = &self.0.store;
        let theirs: HashSet<[u8; 32]> = request.proposals.iter().copied().collect();
        let their_approvals: HashSet<VoteRef> = request.approvals.iter().copied().collect();
        let their_receipts: HashSet<[u8; 32]> = request.receipts.iter().copied().collect();
        let their_rejections: HashSet<VoteRef> = request.rejections.iter().copied().collect();
        let mut records = Vec::new();
        for id in store
            .proposal_ids(vault)?
            .into_iter()
            .filter(|id| !theirs.contains(id))
        {
            if let Some((_, body)) = store.proposal(&id)? {
                records.push(decode_body(&body)?);
            }
        }
        for (proposal, slot) in store.approval_refs(vault)? {
            if !their_approvals.contains(&VoteRef { proposal, slot })
                && let Some(body) = store.approval(&proposal, slot)?
            {
                records.push(decode_body(&body)?);
            }
        }
        for (proposal, slot) in store.rejection_refs(vault)? {
            if !their_rejections.contains(&VoteRef { proposal, slot })
                && let Some(body) = store.rejection(&proposal, slot)?
            {
                records.push(decode_body(&body)?);
            }
        }
        for (proposal, tx_hash) in store.receipts(vault)? {
            if !their_receipts.contains(&proposal) {
                records.push(Message::ExecutionReceipt(ExecutionReceipt {
                    proposal,
                    tx_hash,
                }));
            }
        }
        records.truncate(MAX_SYNC_RECORDS);
        Ok(records)
    }

    /// Periodic work: proposal sync with every member and mailbox polling.
    pub(crate) async fn periodic(&self, now: u64) {
        let due: Vec<[u8; 32]> = {
            let state = self.0.state.lock().await;
            state
                .vaults
                .iter()
                .filter(|(_, v)| v.status == ManifestStatus::Active)
                .filter(|(vault, _)| {
                    state.last_sync.get(*vault).is_none_or(|last| {
                        now.saturating_sub(*last) >= self.0.config.sync_interval_secs
                    })
                })
                .map(|(vault, _)| *vault)
                .collect()
        };
        for vault in due {
            if let Err(error) = self.sync(vault).await {
                tracing::debug!(%error, "sync skipped");
            }
        }
        let poll_due = self.0.mailbox.is_some() && {
            let state = self.0.state.lock().await;
            now.saturating_sub(state.last_mailbox_poll) >= self.0.config.mailbox_interval_secs
        };
        if poll_due && let Err(error) = self.poll_mailbox().await {
            tracing::debug!(%error, "mailbox poll failed");
        }
    }
}
