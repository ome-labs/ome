//! The contract's constants (`constants.nr`) and the Aztec 6.0.0 domain separators the payload hash uses.

/// Most members a vault holds.
pub const MAX_SIGNERS: usize = ome_wire::limits::MAX_MEMBERS;
/// Fields per Grumpkin point.
pub const POINT_LEN: usize = 2;
/// Fields per Schnorr signature: `s.lo`, `s.hi`, `e.lo`, `e.hi`.
pub const SIGNATURE_LEN: usize = 4;
/// Auth witness length: signer keys, one signature per slot, then `expires_at`.
pub const WITNESS_LEN: usize = MAX_SIGNERS * (POINT_LEN + SIGNATURE_LEN) + 1;
/// Smallest viewing threshold: no single member may read the vault.
pub const MIN_VIEWING_THRESHOLD: u32 = 2;
/// Seconds after the anchor block within which a transaction must be included.
pub const CONFIG_MAX_AGE: u64 = 3600;

/// Approval message tag.
pub const APPROVAL_TAG: &str = "ome.approval.v0";
/// Signer-set commitment tag.
pub const SIGNER_SET_TAG: &str = "ome.signers.v0";
/// Committee commitment tag.
pub const COMMITTEE_TAG: &str = "ome.committee.v0";
/// Rejection message tag (off chain only: the contract counts approvals).
pub const REJECTION_TAG: &str = "ome.rejection.v0";

/// `rotate_membership`'s signature, as Aztec derives it from the contract ABI.
pub const ROTATE_MEMBERSHIP_SIGNATURE: &str = "rotate_membership([(Field,Field);8],[(Field,Field);8],u32,u32,u32,[(Field,Field);8],[(Field,Field);8])";
/// `rotate_membership`'s selector (pinned in the contract's `call_layout` test too).
pub const ROTATE_MEMBERSHIP_SELECTOR: u32 = 0xe107_1b7c;
/// Argument fields of `rotate_membership`: current keys, new keys, new count, `k`, `t_v`, committee shares, then the
/// committee polynomial's Feldman commitments.
pub const ROTATE_MEMBERSHIP_ARGS: usize = 4 * MAX_SIGNERS * POINT_LEN + 3;

/// `set_threshold`'s signature, as Aztec derives it from the contract ABI.
pub const SET_THRESHOLD_SIGNATURE: &str = "set_threshold(u32)";
/// `set_threshold`'s selector (checked against Aztec's derivation in `tests/threshold_change.rs`).
pub const SET_THRESHOLD_SELECTOR: u32 = 0x6993_ec6f;

/// Calls the account entrypoint executes (`ACCOUNT_MAX_CALLS`).
pub const APP_MAX_CALLS: usize = 5;
/// Fields per encoded entrypoint call.
pub const CALL_FIELDS: usize = 6;
/// Aztec `DomainSeparator.SIGNATURE_PAYLOAD`.
pub const DOM_SEP_SIGNATURE_PAYLOAD: u64 = 463_525_807;
/// Aztec `DomainSeparator.FUNCTION_ARGS`.
pub const DOM_SEP_FUNCTION_ARGS: u64 = 3_576_554_347;
/// Aztec `DomainSeparator.PUBLIC_CALLDATA`.
pub const DOM_SEP_PUBLIC_CALLDATA: u64 = 2_760_353_947;
/// Aztec `DomainSeparator.AUTHWIT_OUTER`.
pub const DOM_SEP_AUTHWIT_OUTER: u64 = 3_283_595_782;
