//! Execution claims: which member is executing a proposal right now, so the others wait instead of proving the same
//! transaction. A claim is refreshed on every phase change, ends with `mined` or `failed` or the proposal's receipt,
//! and lapses after [`CLAIM_TTL_SECS`] without an update (a member whose daemon died). It is a courtesy, not a lock:
//! the proposal's nonce is what lets only one execution land.

use ome_api::{daemon::ExecutionPhase, keeper::KeeperEvent};
use ome_wire::{
    Header, IdentityPublic, Message,
    messages::proposals::{ClaimPhase, ExecutionClaim},
};

use crate::{Result, keeper::Keeper};

/// How long a claim holds without an update. Proving a vault transaction takes about a minute on a laptop.
pub const CLAIM_TTL_SECS: u64 = 600;

/// A claim this keeper holds: how far the member is, and when it last heard.
pub(crate) struct ClaimRecord {
    phase: ClaimPhase,
    started_at: u64,
    updated_at: u64,
}

/// Claims by vault, proposal and the claiming member's keeper.
pub(crate) type Claims =
    std::collections::HashMap<([u8; 32], [u8; 32], IdentityPublic), ClaimRecord>;

impl Keeper {
    /// Announces that this keeper's member is executing `proposal`, or how far it is; `mined` or `failed` ends it.
    ///
    /// # Errors
    /// [`crate::Error::UnknownVault`].
    pub async fn put_claim(
        &self,
        vault: [u8; 32],
        proposal: [u8; 32],
        phase: ExecutionPhase,
        started_at: u64,
    ) -> Result<()> {
        self.active_vault(vault).await?;
        let claim = ExecutionClaim {
            proposal,
            phase: claim_phase(phase),
            started_at,
        };
        self.record_claim(vault, self.identity(), claim, self.now())
            .await;
        self.broadcast(vault, Message::ExecutionClaim(claim)).await
    }

    /// Records a member's claim. Claims are not replicated records: they pass the freshness window, so a late one is
    /// dropped before it gets here.
    pub(crate) async fn on_claim(
        &self,
        from: IdentityPublic,
        header: Header,
        claim: ExecutionClaim,
    ) {
        let updated_at = header.created_at.min(self.now());
        self.record_claim(header.scope, from, claim, updated_at)
            .await;
    }

    /// The live claims of `vault`.
    pub async fn claims(&self, vault: [u8; 32]) -> Vec<ome_api::keeper::Claim> {
        let now = self.now();
        let me = self.identity();
        let mut state = self.0.state.lock().await;
        state
            .claims
            .retain(|_, c| now.saturating_sub(c.updated_at) <= CLAIM_TTL_SECS);
        let Some(v) = state.vaults.get(&vault) else {
            return Vec::new();
        };
        state
            .claims
            .iter()
            .filter(|((claimed, _, _), _)| *claimed == vault)
            .filter_map(|((_, proposal, member), c)| {
                Some(ome_api::keeper::Claim {
                    proposal: (*proposal).into(),
                    member: v.committee.member(member)?.index,
                    phase: execution_phase(c.phase),
                    started_at: c.started_at,
                    updated_at: c.updated_at,
                    mine: *member == me,
                })
            })
            .collect()
    }

    /// Ends every claim on `proposal`: its receipt arrived.
    pub(crate) async fn end_claims(&self, vault: [u8; 32], proposal: [u8; 32]) {
        self.0
            .state
            .lock()
            .await
            .claims
            .retain(|(v, p, _), _| !(*v == vault && *p == proposal));
    }

    async fn record_claim(
        &self,
        vault: [u8; 32],
        member: IdentityPublic,
        claim: ExecutionClaim,
        updated_at: u64,
    ) {
        let index = {
            let mut state = self.0.state.lock().await;
            let Some(index) = state
                .vaults
                .get(&vault)
                .and_then(|v| v.committee.member(&member))
                .map(|m| m.index)
            else {
                return;
            };
            let key = (vault, claim.proposal, member);
            if claim.phase.is_final() {
                state.claims.remove(&key);
            } else {
                state.claims.insert(
                    key,
                    ClaimRecord {
                        phase: claim.phase,
                        started_at: claim.started_at,
                        updated_at,
                    },
                );
            }
            index
        };
        self.emit(KeeperEvent::Claim {
            vault: vault.into(),
            proposal: claim.proposal.into(),
            member: index,
            phase: execution_phase(claim.phase),
        });
    }
}

const fn claim_phase(phase: ExecutionPhase) -> ClaimPhase {
    match phase {
        ExecutionPhase::Rebuilding => ClaimPhase::Rebuilding,
        ExecutionPhase::Proving => ClaimPhase::Proving,
        ExecutionPhase::Sending => ClaimPhase::Sending,
        ExecutionPhase::Mined => ClaimPhase::Mined,
        ExecutionPhase::Failed => ClaimPhase::Failed,
    }
}

const fn execution_phase(phase: ClaimPhase) -> ExecutionPhase {
    match phase {
        ClaimPhase::Rebuilding => ExecutionPhase::Rebuilding,
        ClaimPhase::Proving => ExecutionPhase::Proving,
        ClaimPhase::Sending => ExecutionPhase::Sending,
        ClaimPhase::Mined => ExecutionPhase::Mined,
        ClaimPhase::Failed => ExecutionPhase::Failed,
    }
}
