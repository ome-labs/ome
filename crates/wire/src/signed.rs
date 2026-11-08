use serde::{Deserialize, Serialize};

use crate::{
    Error, IdentityPublic, KeeperIdentity, Message,
    codec::{canonical, to_canonical},
    limits::MAX_MESSAGE_BYTES,
    types::Bytes64,
    version,
};

/// Routing and replay metadata of every message. All of it is signed, so a relay cannot move a message to another
/// vault or epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// What the message belongs to: the vault address, or a session or lobby id before the vault exists.
    pub scope: [u8; 32],
    /// Committee epoch: the vault config version at which the current viewing committee became active (0 for a DKG,
    /// the old epoch during a reshare).
    pub epoch: u32,
    /// Unique per sender; receivers reject repeats.
    pub id: [u8; 16],
    /// Sender's unix time in seconds; receivers reject values outside their window.
    pub created_at: u64,
}

/// A message body with its header, signed by the sending keeper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedMessage {
    /// Protocol version the message is written in.
    pub version: u16,
    /// Routing and replay metadata.
    pub header: Header,
    /// The message.
    pub body: Message,
    /// The sending keeper.
    pub signer: IdentityPublic,
    /// Ed25519 signature over `"ome.wire.v0" ‖ canonical(version, header, body, signer)`.
    pub signature: Bytes64,
}

impl SignedMessage {
    /// Signs `body` under `header` in protocol `version` (the vault's agreed version).
    ///
    /// # Errors
    /// [`Error::UnsupportedVersion`], bounds errors from [`Message::validate`], or [`Error::TooLarge`] if the signed
    /// message would not fit [`MAX_MESSAGE_BYTES`].
    pub fn sign(
        identity: &KeeperIdentity,
        version: u16,
        header: Header,
        body: Message,
    ) -> Result<Self, Error> {
        version::require_supported(version)?;
        body.validate()?;
        let signer = identity.public();
        let signature = identity.sign(&canonical(&(version, &header, &body, &signer))?);
        let message = Self {
            version,
            header,
            body,
            signer,
            signature,
        };
        to_canonical(&message, MAX_MESSAGE_BYTES)?;
        Ok(message)
    }

    /// Verifies the signature against the claimed signer.
    ///
    /// # Errors
    /// [`Error::InvalidSignature`].
    pub fn verify(&self) -> Result<(), Error> {
        let payload = canonical(&(self.version, &self.header, &self.body, &self.signer))?;
        self.signer.verify(&payload, &self.signature)
    }

    /// Verifies the signature and that the signer is `expected`, e.g. the manifest entry of the claimed member.
    ///
    /// # Errors
    /// [`Error::UnexpectedSigner`] or [`Error::InvalidSignature`].
    pub fn verify_from(&self, expected: &IdentityPublic) -> Result<(), Error> {
        if &self.signer != expected {
            return Err(Error::UnexpectedSigner);
        }
        self.verify()
    }
}
