//! What each member's keeper speaks: its protocol versions and its release, per vault. A keeper signs its own record,
//! sends it on start and whenever it changes, and the vault's keepers replicate it like renames, the newest per member
//! counting. From these records every keeper of a vault computes the same protocol version to write in
//! ([`crate::version::vault_version`]) and can name the members holding a newer version back.

use serde::{Deserialize, Serialize};

use super::bounded;
use crate::{
    Error, Extensions, IdentityPublic, KeeperIdentity, codec::canonical, types::Bytes64,
    version::VersionRange,
};

/// Domain of a capabilities record's signature.
const CAPABILITIES_DOMAIN: &str = "ome.capabilities.v0";

/// Longest release string (`0.2.0`, `0.3.0-rc.1`).
pub const MAX_RELEASE_BYTES: usize = 32;

/// A member's keeper's protocol versions and release in one vault, signed by `by`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// The vault.
    pub vault: [u8; 32],
    /// The protocol versions the keeper reads and writes.
    pub versions: VersionRange,
    /// The keeper's release, shown to the other members.
    pub release: String,
    /// When (the signer's unix seconds).
    pub at: u64,
    /// The member's keeper.
    pub by: IdentityPublic,
    /// Additions a later release may make (none yet), signed with the rest.
    pub extensions: Extensions,
    /// Ed25519 signature by `by` over the domain and the fields above.
    pub signature: Bytes64,
}

impl Capabilities {
    /// `identity`'s record for `vault`.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] for an inverted range or a long release.
    pub fn sign(
        identity: &KeeperIdentity,
        vault: [u8; 32],
        versions: VersionRange,
        release: String,
        at: u64,
    ) -> Result<Self, Error> {
        let by = identity.public();
        let extensions = Extensions::none();
        let signature = identity.sign(&canonical(&(
            CAPABILITIES_DOMAIN,
            vault,
            versions,
            &release,
            at,
            by,
            &extensions,
        ))?);
        let record = Self {
            vault,
            versions,
            release,
            at,
            by,
            extensions,
            signature,
        };
        record.validate()?;
        Ok(record)
    }

    /// Checks the bounds and that `by` signed it.
    ///
    /// # Errors
    /// Bounds errors or [`Error::InvalidSignature`].
    pub fn verify(&self) -> Result<(), Error> {
        self.validate()?;
        self.by.verify(
            &canonical(&(
                CAPABILITIES_DOMAIN,
                self.vault,
                self.versions,
                &self.release,
                self.at,
                self.by,
                &self.extensions,
            ))?,
            &self.signature,
        )
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        self.extensions.validate()?;
        self.versions.validate()?;
        bounded("release", self.release.len(), 1, MAX_RELEASE_BYTES)
    }

    /// Whether this record supersedes `other` from the same member: later.
    #[must_use]
    pub fn supersedes(&self, other: &Self) -> bool {
        self.at > other.at
    }
}
