//! What each member's keeper speaks in a vault, and so the protocol version the vault's keepers write in. This keeper
//! signs its own record when it differs from the last one it sent (a new release, a new range), stores and replicates
//! it; it keeps every member's newest record, from members of the vault's committee only. The vault's version is the
//! newest every current member speaks ([`version::vault_version`]): a member whose record hasn't arrived holds it at
//! the first version, so nothing is ever written that a member can't read.

use std::sync::PoisonError;

use ome_api::keeper as api;
use ome_wire::{
    Message, decode_body, encode_body,
    messages::capabilities::Capabilities,
    version::{self, SUPPORTED, VersionRange},
};

use crate::{Error, Result, keeper::Keeper, store::ManifestStatus};

impl Keeper {
    /// Signs and replicates this keeper's record for `vault` unless its newest one already says the same.
    pub(crate) async fn announce_capabilities(&self, vault: [u8; 32]) -> Result<()> {
        let mine = self.0.keys.identity.public();
        let current = self.latest_capabilities(vault)?;
        let release = ome_api::RELEASE_VERSION;
        if current
            .iter()
            .any(|c| c.by == mine && c.versions == SUPPORTED && c.release == release)
        {
            return Ok(());
        }
        let record = Capabilities::sign(
            &self.0.keys.identity,
            vault,
            SUPPORTED,
            release.to_owned(),
            self.now(),
        )?;
        self.store_capabilities(vault, &record).await?;
        self.broadcast(vault, Message::Capabilities(record)).await?;
        Ok(())
    }

    /// A replicated record: stored when its keeper signed it and is a member of the vault.
    pub(crate) async fn on_capabilities(&self, record: Capabilities) {
        let vault = record.vault;
        if let Err(error) = self.store_capabilities(vault, &record).await {
            tracing::debug!(%error, "dropped a capabilities record");
        }
    }

    /// Every member's newest record of `vault`.
    pub(crate) fn latest_capabilities(&self, vault: [u8; 32]) -> Result<Vec<Capabilities>> {
        let mut latest: Vec<Capabilities> = Vec::new();
        for body in self.0.store.capabilities(&vault)? {
            let Ok(Message::Capabilities(record)) = decode_body(&body) else {
                continue;
            };
            match latest.iter_mut().find(|other| other.by == record.by) {
                Some(other) if record.supersedes(other) => *other = record,
                Some(_) => {}
                None => latest.push(record),
            }
        }
        Ok(latest)
    }

    /// The protocol version `vault`'s keepers write in, and what each current member speaks (in member order).
    ///
    /// # Errors
    /// [`Error::UnknownVault`], storage errors, or no version every member speaks.
    pub async fn vault_protocol(&self, vault: [u8; 32]) -> Result<api::VaultProtocol> {
        let latest = self.latest_capabilities(vault)?;
        let members = {
            let state = self.0.state.lock().await;
            let committee = &state
                .vaults
                .get(&vault)
                .ok_or(Error::UnknownVault)?
                .committee;
            committee
                .manifest
                .members
                .iter()
                .map(|m| {
                    let record = latest.iter().find(|c| c.by == m.identity);
                    api::MemberProtocol {
                        member: m.index,
                        release: record.map(|c| c.release.clone()),
                        versions: record.map(|c| api::ProtocolRange {
                            min: c.versions.min,
                            max: c.versions.max,
                        }),
                    }
                })
                .collect::<Vec<_>>()
        };
        let ranges = members.iter().map(|m| {
            m.versions.map(|r| VersionRange {
                min: r.min,
                max: r.max,
            })
        });
        Ok(api::VaultProtocol {
            version: version::vault_version(ranges)?,
            members,
        })
    }

    /// Refreshes the version this keeper signs `vault`'s messages in.
    pub(crate) async fn refresh_vault_version(&self, vault: [u8; 32]) -> Result<()> {
        let protocol = self.vault_protocol(vault).await?;
        self.0
            .versions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(vault, protocol.version);
        Ok(())
    }

    /// The members, by index, holding `vault` back from protocol `needed`: for refusing a feature that needs it.
    ///
    /// # Errors
    /// [`Error::Invalid`] naming them, when there are any.
    pub async fn require_protocol(
        &self,
        vault: [u8; 32],
        needed: u16,
        feature: &str,
    ) -> Result<()> {
        let protocol = self.vault_protocol(vault).await?;
        let members: Vec<_> = protocol
            .members
            .iter()
            .map(|m| {
                (
                    m,
                    m.versions.map(|r| VersionRange {
                        min: r.min,
                        max: r.max,
                    }),
                )
            })
            .collect();
        let behind = version::holding_back(needed, &members);
        if behind.is_empty() {
            return Ok(());
        }
        let names = behind
            .iter()
            .map(|m| match &m.release {
                Some(release) => format!("member {} runs {release}", m.member),
                None => format!("member {} runs an older release", m.member),
            })
            .collect::<Vec<_>>()
            .join(", ");
        Err(Error::Invalid(format!(
            "{feature} needs protocol v{needed}, which every member must run: {names}"
        )))
    }

    async fn store_capabilities(&self, vault: [u8; 32], record: &Capabilities) -> Result<()> {
        if record.vault != vault {
            return Err(Error::Invalid(
                "a capabilities record for another vault".into(),
            ));
        }
        record.verify()?;
        let member = {
            let state = self.0.state.lock().await;
            state
                .vaults
                .get(&vault)
                .filter(|v| v.status == ManifestStatus::Active)
                .ok_or(Error::UnknownVault)?
                .committee
                .member(&record.by)
                .is_some()
        };
        if !member {
            return Err(Error::Invalid(
                "a capabilities record from outside the committee".into(),
            ));
        }
        let body = encode_body(&Message::Capabilities(record.clone()))?;
        self.0
            .store
            .put_capabilities(&vault, &record.by.0, record.at, &body)?;
        self.refresh_vault_version(vault).await
    }
}
