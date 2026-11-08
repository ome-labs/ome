//! Vault custody: binding a finished DKG to its vault, and reacting to the configs the daemon observes on chain.

use std::sync::Arc;

use ome_api::keeper::{KeeperEvent, Observation};
use ome_crypto::{Point, threshold::ecdh::AddressSecretShare};
use ome_protocol::{membership::Vault as ProtocolVault, vault::Config};
use ome_wire::{
    IdentityPublic,
    messages::ceremony::Manifest,
    types::{WirePoint, parse_field},
};
use serde::Serialize;

use super::ceremonies::ReshareResult;
use crate::{
    Error, Result,
    committee::Committee,
    custody::{ShareSlot, StoredShare},
    keeper::{Keeper, State, VaultState},
    store::{ManifestStatus, from_json},
};

/// A vault as this keeper sees it.
#[derive(Clone, Debug, Serialize)]
pub struct VaultInfo {
    /// Current manifest.
    pub manifest: Manifest,
    /// `active`, `retired` or `removed`.
    pub status: &'static str,
    /// This keeper's member index, if a member.
    pub index: Option<u32>,
    /// Whether this keeper holds its share for the current epoch.
    pub serving: bool,
}

impl Keeper {
    pub(crate) fn load_vaults(&self) -> Result<()> {
        let mut vaults = Vec::new();
        for (manifest, status) in self.0.store.vaults()? {
            let committee = Committee::new(manifest)?;
            let share = if status == ManifestStatus::Active {
                self.load_share(&committee)?
            } else {
                None
            };
            vaults.push((
                committee.vault_bytes(),
                VaultState {
                    committee,
                    status,
                    share,
                },
            ));
        }
        let mut state = self
            .0
            .state
            .try_lock()
            .map_err(|_| Error::Storage("state busy at start".into()))?;
        state.vaults.extend(vaults);
        self.refresh_allowlist(&state);
        Ok(())
    }

    fn load_share(&self, committee: &Committee) -> Result<Option<Arc<AddressSecretShare>>> {
        let Some(member) = committee.member(&self.identity()) else {
            return Ok(None);
        };
        let share = StoredShare::load(
            &self.0.secrets,
            ShareSlot::Active(committee.vault_bytes(), committee.epoch()),
        )?;
        if share.index() != member.index
            || share.group_public_key() != &Point::try_from(&committee.manifest.group_key)?
        {
            return Err(Error::Invalid(
                "stored share does not belong to this committee".into(),
            ));
        }
        Ok(Some(Arc::new(AddressSecretShare::new(
            member.index,
            share.share(),
            &committee.context,
        )?)))
    }

    /// Binds a finished DKG to its vault: checks the manifest (structure, address, verification shares) against the
    /// DKG result and activates the share.
    ///
    /// # Errors
    /// [`Error::Invalid`] if the manifest does not match the DKG, custody or storage errors.
    pub async fn register_vault(&self, session: [u8; 32], manifest: Manifest) -> Result<VaultInfo> {
        let committee = Committee::new(manifest)?;
        let me = committee
            .member(&self.identity())
            .ok_or_else(|| Error::Invalid("this keeper is not in the manifest".into()))?;
        let share = StoredShare::load(&self.0.secrets, ShareSlot::Dkg(session))?;
        let shares: Vec<WirePoint> = share
            .verification_shares()?
            .iter()
            .map(WirePoint::from)
            .collect();
        let manifest_shares: Vec<WirePoint> = committee
            .manifest
            .members
            .iter()
            .map(|m| m.verification_share)
            .collect();
        if share.index() != me.index
            || share.threshold() != committee.threshold()
            || WirePoint::from(share.group_public_key()) != committee.manifest.group_key
            || shares != manifest_shares
        {
            return Err(Error::Invalid(
                "manifest does not match the DKG result".into(),
            ));
        }
        let vault = committee.vault_bytes();
        let mut state = self.0.state.lock().await;
        if state.vaults.contains_key(&vault) {
            return Err(Error::Invalid("vault already registered".into()));
        }
        self.0.secrets.rename(
            &ShareSlot::Dkg(session).name(),
            &ShareSlot::Active(vault, committee.epoch()).name(),
        )?;
        self.0
            .store
            .put_manifest(&committee.manifest, ManifestStatus::Active, self.now())?;
        let address_share = AddressSecretShare::new(me.index, share.share(), &committee.context)?;
        drop(share);
        let epoch = committee.epoch();
        state.vaults.insert(
            vault,
            VaultState {
                committee,
                status: ManifestStatus::Active,
                share: Some(Arc::new(address_share)),
            },
        );
        self.refresh_allowlist(&state);
        let info = Self::info(&state, &vault, &self.identity()).ok_or(Error::UnknownVault)?;
        drop(state);
        self.emit(KeeperEvent::CommitteeActivated {
            vault: vault.into(),
            epoch,
        });
        Ok(info)
    }

    /// Reacts to the vault config its daemon observed on chain.
    ///
    /// The keeper acts only on a config it can verify against its own records:
    ///
    /// - the committee it already serves: nothing changes;
    /// - the committee one of its own completed reshares produced, with a manifest that passes
    ///   `ome_protocol::membership::Vault::from_manifest` against the whole config: the new share becomes active, and
    ///   only then is the old one destroyed;
    /// - the committee of its own completed reshare that leaves this keeper out: its share is destroyed;
    /// - anything else: the share is kept, nothing changes, and [`KeeperEvent::ConfigMismatch`] is raised. Holding a share
    ///   that is no longer needed is recoverable; destroying one that is still needed is not.
    ///
    /// # Errors
    /// Custody or storage errors.
    pub async fn observe_config(&self, vault: [u8; 32], config: Config) -> Result<Observation> {
        let now = self.now();
        self.0.store.observe(&vault, &config, now)?;
        let mut state = self.0.state.lock().await;
        if let Some(current) = state
            .vaults
            .get(&vault)
            .filter(|v| v.status == ManifestStatus::Active)
        {
            if current.committee.commitment() == config.committee_commitment {
                return Ok(Observation::Current);
            }
            if config.version <= current.committee.epoch() {
                return Ok(Observation::Outdated);
            }
        }
        let mut reason = "the committee matches no reshare this keeper took part in".to_owned();
        for (session, result) in self.0.store.completed_reshares(&vault)? {
            let result: ReshareResult = from_json(&result)?;
            if parse_field(&result.committee_commitment)? != config.committee_commitment {
                continue;
            }
            let Some(mut manifest) = result.manifest else {
                // This keeper dealt and leaves: the rotation it took part in has landed.
                self.remove(&mut state, vault, session)?;
                return Ok(Observation::Removed);
            };
            manifest.epoch = config.version;
            match ProtocolVault::from_manifest(&manifest, &config) {
                Ok(_) => {
                    self.activate(&mut state, session, Committee::new(manifest)?)?;
                    return Ok(Observation::Activated);
                }
                Err(error) => {
                    reason =
                        format!("the reshare's committee matches but the config does not: {error}");
                }
            }
        }
        drop(state);
        tracing::warn!(vault = %hex::encode(vault), version = config.version, %reason, "unverifiable config");
        self.emit(KeeperEvent::ConfigMismatch {
            vault: vault.into(),
            version: config.version,
            reason,
        });
        Ok(Observation::Unknown)
    }

    /// Destroys a share this keeper kept because it could not verify the config that replaced its committee: the
    /// member has looked at the alarm and decided the share is no longer needed. Refused while the chain still names
    /// the committee the share serves, so a mistaken call cannot break a working vault, unless `force`.
    ///
    /// `force` is for a member removed by a rotation its keeper took no part in: the new config is encrypted to the new
    /// committee, so this member cannot read it to show the keeper. A member may always destroy its own share; doing so
    /// costs only its own access.
    ///
    /// # Errors
    /// [`Error::UnknownVault`], [`Error::Invalid`] while the committee is still current and not `force`, custody or
    /// storage errors.
    pub async fn retire(&self, vault: [u8; 32], force: bool) -> Result<VaultInfo> {
        let mut state = self.0.state.lock().await;
        let current = state.vaults.get_mut(&vault).ok_or(Error::UnknownVault)?;
        let epoch = current.committee.epoch();
        let superseded = self
            .0
            .store
            .latest_observed_version(&vault)?
            .is_some_and(|version| version > epoch);
        if current.status != ManifestStatus::Active || !(superseded || force) {
            return Err(Error::Invalid(
                "the chain has not moved past the committee this share serves".into(),
            ));
        }
        current.share = None;
        current.status = ManifestStatus::Removed;
        self.0
            .secrets
            .destroy(&ShareSlot::Active(vault, epoch).name())?;
        self.0
            .store
            .set_manifest_status(&vault, epoch, ManifestStatus::Removed, self.now())?;
        self.refresh_allowlist(&state);
        let info = Self::info(&state, &vault, &self.identity()).ok_or(Error::UnknownVault)?;
        drop(state);
        self.emit(KeeperEvent::Retired {
            vault: vault.into(),
        });
        Ok(info)
    }

    /// Destroys the pending shares of reshares that completed more than the configured lifetime ago and never became
    /// active.
    pub(crate) fn expire_pending(&self, now: u64) {
        let before = now.saturating_sub(self.0.config.pending_share_ttl_secs);
        let stale = match self.0.store.stale_reshares(before) {
            Ok(stale) => stale,
            Err(error) => {
                tracing::error!(%error, "cannot list stale reshares");
                return;
            }
        };
        for (session, vault) in stale {
            let slot = ShareSlot::Pending(session).name();
            let outcome = (|| {
                if self.0.secrets.exists(&slot) {
                    self.0.secrets.destroy(&slot)?;
                }
                self.0
                    .store
                    .update_ceremony(&session, "expired", None::<&()>, now)
            })();
            match outcome {
                Ok(()) => {
                    self.emit_ceremony(session, "reshare", "expired");
                    self.emit(KeeperEvent::PendingExpired {
                        vault: vault.into(),
                        session: session.into(),
                    });
                }
                Err(error) => tracing::error!(%error, "cannot expire a pending share"),
            }
        }
    }

    fn activate(&self, state: &mut State, session: [u8; 32], committee: Committee) -> Result<()> {
        let vault = committee.vault_bytes();
        let epoch = committee.epoch();
        let me = committee
            .member(&self.identity())
            .ok_or_else(|| Error::Invalid("not in the new committee".into()))?;
        self.0.secrets.rename(
            &ShareSlot::Pending(session).name(),
            &ShareSlot::Active(vault, epoch).name(),
        )?;
        let share = StoredShare::load(&self.0.secrets, ShareSlot::Active(vault, epoch))?;
        let address_share = AddressSecretShare::new(me.index, share.share(), &committee.context)?;
        drop(share);
        self.0
            .store
            .put_manifest(&committee.manifest, ManifestStatus::Active, self.now())?;
        self.0
            .store
            .set_manifest_status(&vault, epoch, ManifestStatus::Active, self.now())?;
        self.0
            .store
            .update_ceremony(&session, "activated", None::<&()>, self.now())?;
        let replaced = state.vaults.insert(
            vault,
            VaultState {
                committee,
                status: ManifestStatus::Active,
                share: Some(Arc::new(address_share)),
            },
        );
        // The new share is active and stored; only now is the superseded one destroyed.
        if let Some(old) = replaced.filter(|old| old.share.is_some()) {
            self.0
                .secrets
                .destroy(&ShareSlot::Active(vault, old.committee.epoch()).name())?;
        }
        self.refresh_allowlist(state);
        self.emit_ceremony(session, "reshare", "activated");
        self.emit(KeeperEvent::CommitteeActivated {
            vault: vault.into(),
            epoch,
        });
        Ok(())
    }

    fn remove(&self, state: &mut State, vault: [u8; 32], session: [u8; 32]) -> Result<()> {
        let Some(current) = state.vaults.get_mut(&vault) else {
            return Ok(());
        };
        let epoch = current.committee.epoch();
        current.share = None;
        current.status = ManifestStatus::Removed;
        self.0
            .secrets
            .destroy(&ShareSlot::Active(vault, epoch).name())?;
        self.0
            .store
            .set_manifest_status(&vault, epoch, ManifestStatus::Removed, self.now())?;
        self.0
            .store
            .update_ceremony(&session, "activated", None::<&()>, self.now())?;
        self.refresh_allowlist(state);
        self.emit(KeeperEvent::Removed {
            vault: vault.into(),
        });
        Ok(())
    }

    /// A vault's current state.
    ///
    /// # Errors
    /// [`Error::UnknownVault`].
    pub async fn vault(&self, vault: [u8; 32]) -> Result<VaultInfo> {
        let state = self.0.state.lock().await;
        Self::info(&state, &vault, &self.identity()).ok_or(Error::UnknownVault)
    }

    /// Every vault this keeper knows.
    pub async fn vaults(&self) -> Vec<VaultInfo> {
        let state = self.0.state.lock().await;
        let me = self.identity();
        state
            .vaults
            .keys()
            .filter_map(|v| Self::info(&state, v, &me))
            .collect()
    }

    pub(crate) fn info(state: &State, vault: &[u8; 32], me: &IdentityPublic) -> Option<VaultInfo> {
        let v = state.vaults.get(vault)?;
        Some(VaultInfo {
            manifest: v.committee.manifest.clone(),
            status: match v.status {
                ManifestStatus::Active => "active",
                ManifestStatus::Retired => "retired",
                ManifestStatus::Removed => "removed",
            },
            index: v.committee.member(me).map(|m| m.index),
            serving: v.share.is_some(),
        })
    }
}
