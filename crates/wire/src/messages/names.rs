//! Names every member of a vault sees: the vault's own, and each member's display name. A rename is a record the
//! renaming member's keeper signs, replicated to the vault's keepers like votes; the newest per subject wins (by `at`,
//! then by signer as a tie-break), and older ones stay as the history.

use serde::{Deserialize, Serialize};

use super::bounded;
use crate::{
    Error, Extensions, IdentityPublic, KeeperIdentity, codec::canonical, limits::MAX_NAME_BYTES,
    types::Bytes64,
};

/// Domain of a rename's signature.
const NAME_DOMAIN: &str = "ome.name.v0";

/// What a rename names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NameSubject {
    /// The vault.
    Vault,
    /// The member whose keeper signs it: a member renames only themselves.
    Member,
}

/// A rename, signed by `by`, the renaming member's keeper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameChange {
    /// The vault.
    pub vault: [u8; 32],
    /// What it names.
    pub subject: NameSubject,
    /// The new name.
    pub name: String,
    /// When (the signer's unix seconds).
    pub at: u64,
    /// The renaming member's keeper.
    pub by: IdentityPublic,
    /// Additions a later release may make (none yet), signed with the rest.
    pub extensions: Extensions,
    /// Ed25519 signature by `by` over the domain and the fields above.
    pub signature: Bytes64,
}

impl NameChange {
    /// A rename of `subject` in `vault` to `name`, signed by `identity`.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] for an empty or long name.
    pub fn sign(
        identity: &KeeperIdentity,
        vault: [u8; 32],
        subject: NameSubject,
        name: String,
        at: u64,
    ) -> Result<Self, Error> {
        bounded("name", name.len(), 1, MAX_NAME_BYTES)?;
        let by = identity.public();
        let extensions = Extensions::none();
        let signature = identity.sign(&canonical(&(
            NAME_DOMAIN,
            vault,
            subject,
            &name,
            at,
            by,
            &extensions,
        ))?);
        Ok(Self {
            vault,
            subject,
            name,
            at,
            by,
            extensions,
            signature,
        })
    }

    /// Checks the bounds and that `by` signed it.
    ///
    /// # Errors
    /// Bounds errors or [`Error::InvalidSignature`].
    pub fn verify(&self) -> Result<(), Error> {
        self.validate()?;
        self.by.verify(
            &canonical(&(
                NAME_DOMAIN,
                self.vault,
                self.subject,
                &self.name,
                self.at,
                self.by,
                &self.extensions,
            ))?,
            &self.signature,
        )
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        self.extensions.validate()?;
        bounded("name", self.name.len(), 1, MAX_NAME_BYTES)
    }

    /// Whether this rename supersedes `other` of the same subject: later, or as late and signed by a larger identity.
    #[must_use]
    pub fn supersedes(&self, other: &Self) -> bool {
        (self.at, self.by) > (other.at, other.by)
    }
}
