use ome_crypto::threshold::shamir::MemberIndex;
use ome_wire::IdentityPublic;

use crate::keeper::Keeper;
use ome_net::PeerStatus;

use crate::{Error, Result};

/// How reachable one member of a vault's committee is from this keeper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberReachability {
    /// Member index (1-based).
    pub index: MemberIndex,
    /// Keeper identity.
    pub identity: IdentityPublic,
    /// Whether the member is this keeper (then `connected` and nothing else is set).
    pub you: bool,
    /// What the transport knows of the member.
    pub status: PeerStatus,
}

impl Keeper {
    /// Reachability of every member of `vault`'s committee, in index order, this keeper included.
    ///
    /// # Errors
    /// [`Error::UnknownVault`].
    pub async fn peer_status(&self, vault: [u8; 32]) -> Result<Vec<MemberReachability>> {
        let members: Vec<(MemberIndex, IdentityPublic)> = {
            let state = self.0.state.lock().await;
            let v = state.vaults.get(&vault).ok_or(Error::UnknownVault)?;
            v.committee
                .manifest
                .members
                .iter()
                .map(|m| (m.index, m.identity))
                .collect()
        };
        let me = self.identity();
        let others: Vec<IdentityPublic> = members
            .iter()
            .map(|(_, identity)| *identity)
            .filter(|identity| *identity != me)
            .collect();
        let mut statuses = self.0.transport.peer_status(&others).into_iter();
        Ok(members
            .into_iter()
            .map(|(index, identity)| {
                let you = identity == me;
                let status = if you {
                    PeerStatus {
                        connected: true,
                        ..PeerStatus::default()
                    }
                } else {
                    statuses.next().unwrap_or_default()
                };
                MemberReachability {
                    index,
                    identity,
                    you,
                    status,
                }
            })
            .collect())
    }
}
