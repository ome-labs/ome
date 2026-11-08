//! Protocol constants. Lengths and domain separators are a wire format shared with the Rust and TypeScript encoders:
//! changing one changes every commitment and approval message.

/// Maximum number of members (signers and viewing-committee members) a vault can hold.
///
/// Every authorization check verifies one signature slot per member, so proving cost grows linearly with this value.
/// The constructor stores one note per member plus the config note, so 15 is the hard ceiling (16 notes per call).
pub global MAX_SIGNERS: u32 = 8;

/// Fields per Grumpkin point (`x`, `y`).
pub global POINT_LEN: u32 = 2;

/// Fields per Schnorr signature (`s.lo`, `s.hi`, `e.lo`, `e.hi`).
pub global SIGNATURE_LEN: u32 = 4;

/// Auth witness length: ordered signer keys, one signature slot per signer, then `expires_at`.
pub global WITNESS_LEN: u32 = MAX_SIGNERS * (POINT_LEN + SIGNATURE_LEN) + 1;

/// Minimum viewing threshold: no single member may hold enough viewing shares to read the vault.
pub global MIN_VIEWING_THRESHOLD: u32 = 2;

/// Seconds after the anchor block by which a transaction authorized against the current config must be included.
///
/// Bounds how long a superseded config can still authorize. The resulting expiration timestamp is public.
pub global CONFIG_MAX_AGE: u64 = 3600;

/// Domain separator of the approval message: ASCII `ome.approval.v0`.
pub global DOM_SEP__APPROVAL: Field = 0x6f6d652e617070726f76616c2e7630;

/// Domain separator of the signer set commitment: ASCII `ome.signers.v0`.
pub global DOM_SEP__SIGNER_SET: Field = 0x6f6d652e7369676e6572732e7630;

/// Domain separator of the viewing committee commitment: ASCII `ome.committee.v0`.
pub global DOM_SEP__COMMITTEE: Field = 0x6f6d652e636f6d6d69747465652e7630;
