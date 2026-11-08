use ome_crypto::{Base, poseidon2};

use super::{Call, call::empty_call_fields};
use crate::{
    Error,
    vault::constants::{APP_MAX_CALLS, CALL_FIELDS, DOM_SEP_SIGNATURE_PAYLOAD},
};

/// The 31 fields of Aztec's `AppPayload`: five calls of six fields each (padded), then the transaction nonce.
///
/// # Errors
/// [`Error::TooManyCalls`], or call encoding errors.
pub fn app_payload_fields(calls: &[Call], tx_nonce: Base) -> Result<Vec<Base>, Error> {
    if calls.len() > APP_MAX_CALLS {
        return Err(Error::TooManyCalls(calls.len()));
    }
    let mut fields = Vec::with_capacity(APP_MAX_CALLS * CALL_FIELDS + 1);
    for call in calls {
        fields.extend_from_slice(&call.fields()?);
    }
    for _ in calls.len()..APP_MAX_CALLS {
        fields.extend_from_slice(&empty_call_fields());
    }
    fields.push(tx_nonce);
    Ok(fields)
}

/// `AppPayload.hash()`: `Poseidon2(SIGNATURE_PAYLOAD, fields)`.
///
/// # Errors
/// As [`app_payload_fields`].
pub fn app_payload_hash(calls: &[Call], tx_nonce: Base) -> Result<Base, Error> {
    Ok(poseidon2::hash_with_separator(
        Base::from(DOM_SEP_SIGNATURE_PAYLOAD),
        &app_payload_fields(calls, tx_nonce)?,
    ))
}
