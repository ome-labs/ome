use aztec::{macros::notes::note, protocol::traits::{Deserialize, Packable, Serialize}};

/// One signer key at a fixed slot of the signer set.
///
/// Signer notes let every member rebuild the ordered signer set from chain data. Authorization never reads them: a
/// `PrivateSet` cannot prove the set is complete, so the witness is checked against the config note's commitment.
#[derive(Deserialize, Eq, Packable, Serialize)]
#[note]
pub struct SignerNote {
    pub x: Field,
    pub y: Field,
    pub slot: u32,
}
