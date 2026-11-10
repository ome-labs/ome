//! Canonical encoding. Every value has exactly one byte form: decoding re-encodes what it parsed and rejects the input
//! unless the bytes match (postcard alone also accepts overlong varints), so signatures and byte-level deduplication
//! are sound.

use serde::{Serialize, de::DeserializeOwned};

use crate::{Error, Message, SignedMessage, limits::MAX_MESSAGE_BYTES, version};

/// Encodes a signed message.
///
/// # Errors
/// [`Error::TooLarge`] above [`MAX_MESSAGE_BYTES`].
pub fn encode(message: &SignedMessage) -> Result<Vec<u8>, Error> {
    to_canonical(message, MAX_MESSAGE_BYTES)
}

/// Decodes and bounds-checks a signed message. Does **not** verify the signature: call [`SignedMessage::verify`] or
/// [`SignedMessage::verify_from`].
///
/// # Errors
/// [`Error::TooLarge`], [`Error::Malformed`] (including non-canonical bytes), [`Error::UnsupportedVersion`] or
/// [`Error::OutOfBounds`].
pub fn decode(bytes: &[u8]) -> Result<SignedMessage, Error> {
    let message: SignedMessage = from_canonical(bytes, MAX_MESSAGE_BYTES)?;
    version::require_supported(message.version)?;
    message.extensions.validate()?;
    message.body.validate()?;
    Ok(message)
}

/// Encodes an unsigned body, as a daemon hands it to its own keeper for signing.
///
/// # Errors
/// [`Error::OutOfBounds`] from [`Message::validate`], [`Error::TooLarge`].
pub fn encode_body(body: &Message) -> Result<Vec<u8>, Error> {
    body.validate()?;
    to_canonical(body, MAX_MESSAGE_BYTES)
}

/// Decodes and bounds-checks an unsigned body.
///
/// # Errors
/// [`Error::TooLarge`], [`Error::Malformed`] or [`Error::OutOfBounds`].
pub fn decode_body(bytes: &[u8]) -> Result<Message, Error> {
    let body: Message = from_canonical(bytes, MAX_MESSAGE_BYTES)?;
    body.validate()?;
    Ok(body)
}

/// Canonical bytes of a value (signing payloads and content ids).
pub(crate) fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    postcard::to_allocvec(value).map_err(|_| Error::Malformed)
}

/// Canonical bytes of any wire value, refused above `limit`. Other Ome formats built on this encoding (such as the
/// proposal payload in `ome-protocol`) use it so they share the one-byte-form rule.
///
/// # Errors
/// [`Error::TooLarge`] above `limit`, [`Error::Malformed`] if the value cannot be encoded.
pub fn to_canonical<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, Error> {
    let bytes = canonical(value)?;
    if bytes.len() > limit {
        return Err(Error::TooLarge(bytes.len()));
    }
    Ok(bytes)
}

/// Parses exactly one canonical value of at most `limit` bytes.
///
/// # Errors
/// [`Error::TooLarge`] above `limit`, [`Error::Malformed`] for invalid, trailing or non-canonical bytes.
pub fn from_canonical<T: Serialize + DeserializeOwned>(
    bytes: &[u8],
    limit: usize,
) -> Result<T, Error> {
    if bytes.len() > limit {
        return Err(Error::TooLarge(bytes.len()));
    }
    let (value, rest): (T, &[u8]) =
        postcard::take_from_bytes(bytes).map_err(|_| Error::Malformed)?;
    if !rest.is_empty() || canonical(&value)? != bytes {
        return Err(Error::Malformed);
    }
    Ok(value)
}
