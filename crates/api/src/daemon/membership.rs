//! Membership changes. A rotation reshares the viewing secret to the new committee, then one proposal runs
//! `rotate_membership`, which changes the signers, thresholds and committee together and removes at most `t_v - 1`
//! members.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Hex32, Point, keeper::KeeperContact};

/// A member to add, as a verified join request names them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Joiner {
    /// Name.
    pub name: String,
    /// The newcomer's own Aztec account.
    pub account: Hex32,
    /// Approval signing key.
    pub signer_key: Point,
    /// Their keeper.
    pub keeper: KeeperContact,
}

/// Params of `membership.plan`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MembershipParams {
    /// Vault.
    pub vault: Hex32,
    /// Member indices to remove.
    pub remove: Vec<u32>,
    /// Join requests (`membership.request` on the newcomers' daemons) of the members to add.
    pub add: Vec<String>,
    /// New spending threshold.
    pub threshold: u32,
    /// New viewing threshold.
    pub viewing_threshold: u32,
}

/// A committee's shape before or after a rotation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeShape {
    /// Spending threshold.
    pub threshold: u32,
    /// Viewing threshold.
    pub viewing_threshold: u32,
    /// Member names in index order.
    pub members: Vec<String>,
}

/// A planned rotation, checked on this device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MembershipPlan {
    /// Rotation id.
    pub id: Hex32,
    /// Vault.
    pub vault: Hex32,
    /// Now.
    pub current: CommitteeShape,
    /// After.
    pub next: CommitteeShape,
    /// Members that deal sub-shares: members who stay.
    pub dealers: Vec<u32>,
    /// The members joining, from their verified join requests.
    pub joiners: Vec<Joiner>,
    /// Whether it removes at most `t_v - 1` members.
    pub within_removal_limit: bool,
    /// Why it cannot run, if it cannot.
    pub failure: Option<String>,
    /// The plan to hand every member who stays and every joiner (`ome-rotation-v0:…`); each passes it to
    /// `membership.begin`. It carries the reshare's parameters and the vault's deployment, no secrets.
    pub share: String,
}

/// Params of `membership.begin`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RotationPlanText {
    /// A plan from `membership.plan`, on this or another member's daemon.
    pub plan: String,
}

/// Params naming a rotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RotationRef {
    /// Vault.
    pub vault: Hex32,
    /// Rotation id.
    pub id: Hex32,
}

/// A rotation step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RotationStepId {
    /// The dealers reshare to the new committee.
    Reshare,
    /// The `rotate_membership` proposal is made.
    Propose,
    /// Members approve it.
    Approve,
    /// It executes.
    Execute,
    /// Keepers observe the new config and switch shares.
    Activate,
}

/// One rotation step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RotationStep {
    /// Which.
    pub id: RotationStepId,
    /// Where it is.
    pub status: super::StepStatus,
    /// Short detail.
    pub detail: Option<String>,
}

/// A rotation in progress or finished.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RotationState {
    /// The plan.
    pub plan: MembershipPlan,
    /// Steps in order.
    pub steps: Vec<RotationStep>,
    /// The reshare session, once started.
    pub session: Option<Hex32>,
    /// The `rotate_membership` proposal, once made.
    pub proposal: Option<Hex32>,
    /// Failure.
    pub error: Option<String>,
    /// Finished, successfully or not.
    pub done: bool,
}
