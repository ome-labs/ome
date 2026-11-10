//! Names every member of a vault sees: the vault's, and each member's display name. Renames are signed by the renaming
//! member's keeper and replicated; the newest per subject wins.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Hex32;

/// What a rename names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum NameSubject {
    /// The vault.
    Vault,
    /// The renaming member themselves.
    Member,
}

/// Params of `names.put`: this keeper's member renames the vault, or themselves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NamePutParams {
    /// Vault.
    pub vault: Hex32,
    /// What it names.
    pub subject: NameSubject,
    /// The new name, 1 to 64 bytes.
    pub name: String,
}

/// The current name of a subject.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Name {
    /// What it names.
    pub subject: NameSubject,
    /// The name.
    pub name: String,
    /// When it was set (the renaming keeper's unix seconds).
    pub at: u64,
    /// The renaming member's keeper identity (for a member's name, the member).
    pub by: Hex32,
    /// The renaming member's index in the current committee, when still a member.
    pub member: Option<u32>,
}
