//! Vaults as the member sees them: summaries, details, peers, activity and tokens.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ProposalView;
use crate::{Amount, Hex32, Hex64, Point};

/// One of the member's vaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct VaultSummary {
    /// Vault address.
    pub address: Hex32,
    /// Name.
    pub name: String,
    /// Local alias, if set.
    pub alias: Option<String>,
    /// The vault commands use when none is named.
    pub is_default: bool,
    /// Spending threshold.
    pub threshold: u32,
    /// Viewing threshold.
    pub viewing_threshold: u32,
    /// Members.
    pub members: u32,
    /// This member's index.
    pub my_index: u32,
    /// Unix seconds it was founded.
    pub founded_at: u64,
}

/// The vault contract's deployment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContractInfo {
    /// Address.
    pub address: Hex32,
    /// Preaddress.
    pub preaddress: Hex32,
    /// Partial address.
    pub partial_address: Hex32,
    /// Contract class id.
    pub class_id: Hex32,
    /// Deployment salt.
    pub salt: Hex32,
    /// Deployer.
    pub deployer: Hex32,
    /// Initialization hash.
    pub initialization_hash: Hex32,
    /// Public keys by name (`ivpk`, `ovpk`, ...), as hex of their point encoding.
    pub public_keys: Vec<NamedKey>,
}

/// A named public key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NamedKey {
    /// Name.
    pub name: String,
    /// Point.
    pub key: Point,
}

/// The vault's config note, read from the chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    /// Config version.
    pub version: u32,
    /// Spending threshold.
    pub threshold: u32,
    /// Viewing threshold.
    pub viewing_threshold: u32,
    /// Registered signers.
    pub signer_count: u32,
    /// Signer-set commitment.
    pub signer_set_commitment: Hex32,
    /// Committee commitment.
    pub committee_commitment: Hex32,
    /// Whether it matches the committee the keeper serves.
    pub matches_committee: bool,
    /// Unix seconds it was read.
    pub read_at: u64,
}

/// A committee member, with the name the member gave themselves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeMember {
    /// 1-based index.
    pub index: u32,
    /// Display name.
    pub name: String,
    /// Approval signing key.
    pub signer_key: Point,
    /// Verification share.
    pub verification_share: Point,
    /// Keeper identity.
    pub identity: Hex32,
    /// Keeper transport key.
    pub transport: Hex32,
    /// libp2p peer id.
    pub peer_id: String,
    /// Keeper addresses.
    pub addresses: Vec<String>,
    /// Whether it is this member.
    pub you: bool,
}

/// The committee the keeper serves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeView {
    /// Epoch.
    pub epoch: u32,
    /// Viewing threshold.
    pub viewing_threshold: u32,
    /// Group key.
    pub group_key: Point,
    /// Members.
    pub members: Vec<CommitteeMember>,
    /// Whether the keeper holds its share.
    pub serving: bool,
    /// `active`, `retired` or `removed`.
    pub keeper_status: String,
}

/// How the vault was founded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FoundingInfo {
    /// DKG session.
    pub session: Hex32,
    /// DKG transcript hash.
    pub transcript: Option<Hex64>,
    /// Deployment transaction.
    pub deploy_tx: Option<Hex32>,
}

/// Everything about one vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VaultDetails {
    /// Summary.
    pub summary: VaultSummary,
    /// Deployment.
    pub contract: ContractInfo,
    /// On-chain config, once read.
    pub config: Option<ConfigView>,
    /// Committee.
    pub committee: CommitteeView,
    /// Founding.
    pub founding: FoundingInfo,
}

/// A member's reachability, with their name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MemberReachability {
    /// Member index.
    pub index: u32,
    /// Display name.
    pub name: String,
    /// Keeper identity.
    pub identity: Hex32,
    /// libp2p peer id.
    pub peer_id: String,
    /// Whether it is this member.
    pub you: bool,
    /// Connected.
    pub connected: bool,
    /// Only through relay circuits.
    pub relayed: bool,
    /// Unix seconds of the last contact.
    pub last_seen: Option<u64>,
    /// Round trip in milliseconds.
    pub rtt_ms: Option<u64>,
}

/// One ECDH audit entry, with the requester's name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditView {
    /// Unix seconds.
    pub at: u64,
    /// `local`, or the requesting keeper's identity.
    pub requester: String,
    /// The requesting member's name, if known.
    pub requester_name: Option<String>,
    /// What the points were for.
    pub purpose: String,
    /// Number of points.
    pub points: u32,
    /// `served`, or why it was refused.
    pub outcome: String,
}

/// A member of a vault, by slot and name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MemberRef {
    /// Member index, from 1.
    pub index: u32,
    /// Name.
    pub name: String,
}

/// A token the member knows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenInfo {
    /// Token contract.
    pub address: Hex32,
    /// Name.
    pub name: String,
    /// Symbol.
    pub symbol: String,
    /// Decimals.
    pub decimals: u8,
}

/// The vault's private balance of a token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenBalance {
    /// Token.
    pub token: TokenInfo,
    /// Balance, if it could be read.
    pub balance: Option<Amount>,
    /// Why it could not.
    pub error: Option<String>,
}

/// Something that happened to a vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ActivityEntry {
    /// A proposal.
    Proposal {
        /// Unix seconds.
        at: u64,
        /// The proposal.
        proposal: Box<ProposalView>,
    },
    /// A private transfer the vault received.
    Received {
        /// Unix seconds.
        at: u64,
        /// Token.
        token: TokenInfo,
        /// Sender, as the note reveals it.
        from: Hex32,
        /// Amount.
        amount: Amount,
        /// Transaction.
        tx_hash: Hex32,
        /// Block.
        block: u64,
    },
}

/// Someone who paid the vault, from the transfers it received.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Counterparty {
    /// Sender.
    pub address: Hex32,
    /// Its name in `aztec-wallet`'s aliases, if any.
    pub label: Option<String>,
    /// Transfers received from it.
    pub received: u32,
    /// First transfer (unix seconds).
    pub first_at: u64,
    /// Latest transfer (unix seconds).
    pub last_at: u64,
    /// It paid the vault before a member was removed: that member can still tell when it pays again, until it
    /// handshakes with the vault anew.
    pub exposed: bool,
}
