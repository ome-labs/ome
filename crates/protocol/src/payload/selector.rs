use ome_crypto::{curve, poseidon2};

use crate::Error;

/// A function selector: the low 4 bytes of `Poseidon2` over the signature's bytes, packed 31 per field the way Noir
/// reads them (little-endian), as `FunctionSelector.fromSignature` computes it.
///
/// # Errors
/// [`Error::InvalidSignatureText`] if the signature contains whitespace.
pub fn selector(signature: &str) -> Result<u32, Error> {
    if signature.chars().any(char::is_whitespace) {
        return Err(Error::InvalidSignatureText);
    }
    let fields = signature
        .as_bytes()
        .chunks(31)
        .map(|chunk| {
            let mut bytes = [0u8; 32];
            for (i, byte) in chunk.iter().enumerate() {
                bytes[31 - i] = *byte;
            }
            curve::base_from_bytes(&bytes)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let hash = curve::base_to_bytes(&poseidon2::hash(&fields));
    Ok(u32::from_be_bytes([hash[28], hash[29], hash[30], hash[31]]))
}
