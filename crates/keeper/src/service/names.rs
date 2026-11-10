//! Names every member of a vault sees: the vault's own, and each member's display name. A rename is signed by the
//! renaming member's keeper, so it names its author however it travels; a keeper stores renames only from members of
//! the vault's committee, keeps every one, and reports the newest per subject.

use ome_api::keeper::{self as api, KeeperEvent};
use ome_wire::{
    Header, Message, decode_body, encode_body,
    messages::names::{NameChange, NameSubject},
};

use crate::{Error, Result, keeper::Keeper, store::ManifestStatus};

const fn subject_label(subject: NameSubject) -> &'static str {
    match subject {
        NameSubject::Vault => "vault",
        NameSubject::Member => "member",
    }
}

const fn api_subject(subject: NameSubject) -> api::NameSubject {
    match subject {
        NameSubject::Vault => api::NameSubject::Vault,
        NameSubject::Member => api::NameSubject::Member,
    }
}

impl Keeper {
    /// Renames `vault`, or this keeper's member in it, for every member: signs, stores and replicates the rename.
    ///
    /// # Errors
    /// [`Error::UnknownVault`], [`Error::Invalid`] for an empty or long name.
    pub async fn put_name(
        &self,
        vault: [u8; 32],
        subject: api::NameSubject,
        name: String,
    ) -> Result<api::Name> {
        self.active_vault(vault).await?;
        let subject = match subject {
            api::NameSubject::Vault => NameSubject::Vault,
            api::NameSubject::Member => NameSubject::Member,
        };
        let change = NameChange::sign(&self.0.keys.identity, vault, subject, name, self.now())
            .map_err(|_| Error::Invalid("a name is 1 to 64 bytes".into()))?;
        self.store_name(vault, &change).await?;
        self.broadcast(vault, Message::NameChange(change.clone()))
            .await?;
        Ok(self.name_view(vault, &change).await)
    }

    /// The vault's current name and each member's, as renamed (nothing until someone renames).
    ///
    /// # Errors
    /// Storage errors.
    pub async fn names(&self, vault: [u8; 32]) -> Result<Vec<api::Name>> {
        let mut views = Vec::new();
        for change in self.latest_names(vault)? {
            views.push(self.name_view(vault, &change).await);
        }
        Ok(views)
    }

    /// A replicated rename: stored when its author's keeper signed it and is a member of the vault.
    pub(crate) async fn on_name(&self, header: &Header, change: NameChange) {
        if change.vault != header.scope {
            return;
        }
        if let Err(error) = self.store_name(change.vault, &change).await {
            tracing::debug!(%error, "dropped a rename");
        }
    }

    /// The newest rename per subject: the vault's, and each member's own.
    pub(crate) fn latest_names(&self, vault: [u8; 32]) -> Result<Vec<NameChange>> {
        let mut latest: Vec<NameChange> = Vec::new();
        for body in self.0.store.names(&vault)? {
            let Ok(Message::NameChange(change)) = decode_body(&body) else {
                continue;
            };
            let same = |other: &NameChange| {
                other.subject == change.subject
                    && (change.subject == NameSubject::Vault || other.by == change.by)
            };
            match latest.iter_mut().find(|other| same(other)) {
                Some(other) if change.supersedes(other) => *other = change,
                Some(_) => {}
                None => latest.push(change),
            }
        }
        Ok(latest)
    }

    async fn store_name(&self, vault: [u8; 32], change: &NameChange) -> Result<()> {
        change.verify()?;
        let member = {
            let state = self.0.state.lock().await;
            state
                .vaults
                .get(&vault)
                .filter(|v| v.status == ManifestStatus::Active)
                .ok_or(Error::UnknownVault)?
                .committee
                .member(&change.by)
                .is_some()
        };
        if !member {
            return Err(Error::Invalid("a rename from outside the committee".into()));
        }
        let body = encode_body(&Message::NameChange(change.clone()))?;
        let stored = self.0.store.put_name(
            &vault,
            subject_label(change.subject),
            &change.by.0,
            change.at,
            &body,
        )?;
        // Announce it only when it is now the name.
        if stored
            && self
                .latest_names(vault)?
                .iter()
                .any(|latest| latest == change)
        {
            let name = self.name_view(vault, change).await;
            self.emit(KeeperEvent::Name {
                vault: vault.into(),
                name,
            });
        }
        Ok(())
    }

    async fn name_view(&self, vault: [u8; 32], change: &NameChange) -> api::Name {
        api::Name {
            subject: api_subject(change.subject),
            name: change.name.clone(),
            at: change.at,
            by: change.by.0.into(),
            member: self.member_index(vault, &change.by).await,
        }
    }
}
