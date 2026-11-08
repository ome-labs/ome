use aztec::{macros::notes::note, protocol::traits::{Deserialize, Packable, Serialize}};

/// Current policy of the vault: spending threshold, signer set and viewing committee.
///
/// Authorization reads only this note. The signer keys and verification shares are committed to, not stored:
/// approvals carry the signer keys in the witness, and keepers carry the verification shares. `version`
/// increases on every config change and is bound into every approval.
#[derive(Deserialize, Eq, Packable, Serialize)]
#[note]
pub struct ConfigNote {
    pub threshold: u32,
    pub signer_count: u32,
    pub version: u32,
    pub signer_set_commitment: Field,
    pub viewing_threshold: u32,
    pub committee_commitment: Field,
}
