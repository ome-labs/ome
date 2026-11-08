//! The mailbox protocol between keepers and an untrusted relay. The relay stores sealed [`RelayEnvelope`]s under
//! opaque 32-byte mailbox ids that only the sender and recipient can derive
//! (`ome_crypto::sealing::mailbox::mailbox_id`); it never sees a plaintext, an identity or a vault address. These
//! messages are not signed: they use the same canonical encoding, with their own limits.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::bounded;
use crate::{
    Error,
    codec::{canonical, from_canonical, to_canonical},
    limits::{
        MAX_DELETE_IDS, MAX_ENVELOPE_BYTES, MAX_FETCH_ENVELOPES, MAX_FETCH_MAILBOXES,
        MAX_MAILBOX_MESSAGE_BYTES,
    },
};

/// Domain tag of envelope ids.
pub const ENVELOPE_DOMAIN: &[u8] = b"ome.mailbox.envelope.v0";

/// Ciphertext a relay holds for an offline recipient. The relay sees only the opaque mailbox id and the expiry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayEnvelope {
    /// Opaque recipient mailbox id.
    pub recipient: [u8; 32],
    /// Drop after this unix time.
    pub expires_at: u64,
    /// Sealed bytes: an encoded, sealed `SignedMessage`.
    pub sealed: Vec<u8>,
}

impl RelayEnvelope {
    /// `SHA-256("ome.mailbox.envelope.v0" ‖ canonical(envelope))`.
    ///
    /// # Errors
    /// [`Error::Malformed`] if the envelope cannot be encoded.
    pub fn id(&self) -> Result<[u8; 32], Error> {
        let mut hasher = Sha256::new();
        hasher.update(ENVELOPE_DOMAIN);
        hasher.update(canonical(self)?);
        Ok(hasher.finalize().into())
    }

    fn validate(&self) -> Result<(), Error> {
        bounded("sealed", self.sealed.len(), 1, MAX_ENVELOPE_BYTES)
    }
}

/// A keeper's request to a relay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailboxRequest {
    /// Store an envelope until it expires or is deleted.
    Deposit(RelayEnvelope),
    /// Return envelopes held for these mailboxes.
    Fetch {
        /// Mailbox ids.
        mailboxes: Vec<[u8; 32]>,
    },
    /// Delete delivered envelopes.
    Delete {
        /// Envelope ids.
        ids: Vec<[u8; 32]>,
    },
}

impl MailboxRequest {
    fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Deposit(envelope) => envelope.validate(),
            Self::Fetch { mailboxes } => {
                bounded("mailboxes", mailboxes.len(), 1, MAX_FETCH_MAILBOXES)
            }
            Self::Delete { ids } => bounded("ids", ids.len(), 1, MAX_DELETE_IDS),
        }
    }
}

/// Why a relay refused a deposit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// Payload above [`MAX_ENVELOPE_BYTES`].
    TooLarge,
    /// Already expired, or expiring beyond the relay's maximum lifetime.
    BadExpiry,
    /// The depositor or the mailbox is over quota.
    QuotaExceeded,
    /// The relay serves only allowlisted keepers.
    NotAllowed,
    /// The request was malformed or out of bounds.
    Malformed,
}

/// A stored envelope with its id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEnvelope {
    /// [`RelayEnvelope::id`].
    pub id: [u8; 32],
    /// The envelope.
    pub envelope: RelayEnvelope,
}

/// A relay's answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailboxResponse {
    /// Stored under this id.
    Stored {
        /// Envelope id.
        id: [u8; 32],
    },
    /// Not stored.
    Refused(Refusal),
    /// Envelopes for the requested mailboxes, possibly only some of them; fetch again after deleting.
    Envelopes(Vec<StoredEnvelope>),
    /// Number of envelopes deleted.
    Deleted(u32),
}

impl MailboxResponse {
    fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Envelopes(list) => {
                bounded("envelopes", list.len(), 0, MAX_FETCH_ENVELOPES)?;
                list.iter()
                    .try_for_each(|stored| stored.envelope.validate())
            }
            Self::Stored { .. } | Self::Refused(_) | Self::Deleted(_) => Ok(()),
        }
    }
}

/// Encodes a request.
///
/// # Errors
/// Bounds errors or [`Error::TooLarge`].
pub fn encode_request(request: &MailboxRequest) -> Result<Vec<u8>, Error> {
    request.validate()?;
    to_canonical(request, MAX_MAILBOX_MESSAGE_BYTES)
}

/// Decodes and bounds-checks a request.
///
/// # Errors
/// [`Error::TooLarge`], [`Error::Malformed`] or [`Error::OutOfBounds`].
pub fn decode_request(bytes: &[u8]) -> Result<MailboxRequest, Error> {
    let request: MailboxRequest = from_canonical(bytes, MAX_MAILBOX_MESSAGE_BYTES)?;
    request.validate()?;
    Ok(request)
}

/// Encodes a response.
///
/// # Errors
/// Bounds errors or [`Error::TooLarge`].
pub fn encode_response(response: &MailboxResponse) -> Result<Vec<u8>, Error> {
    response.validate()?;
    to_canonical(response, MAX_MAILBOX_MESSAGE_BYTES)
}

/// Decodes and bounds-checks a response.
///
/// # Errors
/// [`Error::TooLarge`], [`Error::Malformed`] or [`Error::OutOfBounds`].
pub fn decode_response(bytes: &[u8]) -> Result<MailboxResponse, Error> {
    let response: MailboxResponse = from_canonical(bytes, MAX_MAILBOX_MESSAGE_BYTES)?;
    response.validate()?;
    Ok(response)
}
