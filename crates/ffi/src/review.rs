//! Reviewing a proposal on the phone. The phone verifies everything it can from the proposal itself: the committee
//! against the config, the calls against the hash it signs, each call's signature against its selector, that its key
//! holds a slot, its own policy, and a rotation's or signer swap's shape. It trusts its member's own keeper and daemon
//! only for the config being current, unpinned labels, the simulation and which rotation the keeper reshared.

use ome_crypto::Point;
use ome_protocol::{
    membership::{
        Rotation, Vault, check_rotation, check_signer_swap, is_rotation_call, signer_swap_slot,
    },
    payload::{Call, CallKind, ChainInfo, ExecutionRequest, arguments},
    proposals::check_proposal,
    vault::{Committee, Config, SignerSet},
};
use ome_wire::{
    messages::device::{DeviceReview, DeviceRotation},
    types::{WireField, parse_field, parse_points},
};

use crate::{
    Policy,
    policy::{display_address, field_u128},
};

/// What a proposal does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ReviewKind {
    /// Calls other contracts: a payment or any other action.
    Payment,
    /// Changes the vault's members and viewing committee.
    Rotation,
    /// Moves one member's signing key to another device.
    SignerSwap,
    /// Does nothing but burn the nonce of another proposal.
    Cancellation,
}

/// Whether this phone voted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ReviewStatus {
    /// Not voted yet.
    Pending,
    /// Approved on this phone.
    Approved,
    /// Rejected on this phone.
    Rejected,
}

/// One check the phone ran.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Check {
    /// What was checked.
    pub name: String,
    /// Whether it passed.
    pub passed: bool,
    /// Why it failed.
    pub detail: Option<String>,
}

/// One parameter of a call, decoded by the function signature.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Argument {
    /// The type as the signature writes it.
    pub kind: String,
    /// The value: decimal for integers, `true`/`false`, hex otherwise.
    pub value: String,
}

/// One call, as the phone shows it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct CallView {
    /// Target contract (hex).
    pub to: String,
    /// A name for the target: pinned on this phone, or claimed by the member's daemon.
    pub label: Option<String>,
    /// Whether the label is pinned on this phone.
    pub label_pinned: bool,
    /// The function name, when the call carries its signature.
    pub function: Option<String>,
    /// The arguments.
    pub arguments: Vec<Argument>,
    /// Whether the call is enqueued for public execution.
    pub public: bool,
    /// Whether it pays the fee through a trusted fee payer.
    pub fee_payment: bool,
}

/// A proposal as the phone shows it, with every check it ran.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Review {
    /// Proposal id (hex).
    pub id: String,
    /// Vault address (hex).
    pub vault: String,
    /// The vault's name.
    pub vault_name: String,
    /// The proposer's summary.
    pub summary: String,
    /// What the proposal does.
    pub kind: ReviewKind,
    /// Proposing member (1-based).
    pub proposer: u32,
    /// Approval deadline (unix seconds).
    pub expires_at: u64,
    /// Config version the proposal was made under.
    pub config_version: u32,
    /// Approvals the vault needs.
    pub threshold: u32,
    /// Members of the vault.
    pub signers: u32,
    /// The calls.
    pub calls: Vec<CallView>,
    /// Every check.
    pub checks: Vec<Check>,
    /// The daemon's simulation outcome.
    pub simulation: Option<String>,
    /// Whether every check passed and this phone has not voted: approving is allowed.
    pub signable: bool,
    /// This phone's vote.
    pub status: ReviewStatus,
    /// What the keeper said about the vote: `None` while waiting.
    pub outcome: Option<String>,
}

/// What the phone found, and what signing needs.
pub(crate) struct Analysis {
    pub(crate) kind: ReviewKind,
    pub(crate) checks: Vec<Check>,
    pub(crate) calls: Vec<CallView>,
    pub(crate) vault: Option<Vault>,
    pub(crate) chain: Option<ChainInfo>,
    pub(crate) rotation: Option<Rotation>,
}

impl Analysis {
    pub(crate) fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }
}

fn check(name: &str, result: Result<(), String>) -> Check {
    Check {
        name: name.to_owned(),
        passed: result.is_ok(),
        detail: result.err(),
    }
}

fn config(review: &DeviceReview) -> Result<Config, ome_wire::Error> {
    let c = review.config;
    Ok(Config {
        threshold: c.threshold,
        signer_count: c.signer_count,
        version: c.version,
        signer_set_commitment: parse_field(&c.signer_set_commitment)?,
        viewing_threshold: c.viewing_threshold,
        committee_commitment: parse_field(&c.committee_commitment)?,
    })
}

fn rotation(rotation: &DeviceRotation) -> Result<Rotation, ome_protocol::Error> {
    Ok(Rotation {
        signers: SignerSet::new(parse_points(&rotation.signers)?)?,
        threshold: rotation.threshold,
        committee: Committee::new(
            rotation.viewing_threshold,
            parse_points(&rotation.verification_shares)?,
        )?,
    })
}

/// Runs every check on `review` for a phone signing with `key`.
pub(crate) fn analyze(
    review: &DeviceReview,
    key: &Point,
    policy: &Policy,
    fee_payers: &[WireField],
    now: u64,
) -> Analysis {
    let mut checks = Vec::new();
    let vault = config(review)
        .map_err(|e| e.to_string())
        .and_then(|config| {
            if review.manifest.vault != review.vault {
                return Err("the committee belongs to another vault".to_owned());
            }
            Vault::from_manifest(&review.manifest, &config).map_err(|e| e.to_string())
        });
    checks.push(check(
        "The committee matches the vault's config",
        vault.as_ref().map(drop).map_err(Clone::clone),
    ));
    let vault = vault.ok();
    let chain = parse_field(&review.chain.chain_id)
        .and_then(|chain_id| {
            Ok(ChainInfo {
                chain_id,
                version: parse_field(&review.chain.version)?,
            })
        })
        .ok();
    let request = ExecutionRequest::from_payload(&review.payload);
    checks.push(check(
        "Every call's function matches its selector",
        request.as_ref().map(drop).map_err(ToString::to_string),
    ));
    let rebuilt = match (&vault, chain) {
        (Some(vault), Some(chain)) => {
            check_proposal(vault, chain, &review.proposal, &review.payload, now)
                .map(drop)
                .map_err(|e| e.to_string())
        }
        _ => Err("the vault or chain is not readable".to_owned()),
    };
    checks.push(check(
        "The calls hash to what you sign, under the current config, before the deadline",
        rebuilt,
    ));
    let slot = vault
        .as_ref()
        .and_then(|v| v.signers().keys().iter().position(|k| k == key));
    checks.push(check(
        "This phone holds a signing slot in the vault",
        slot.map(drop)
            .ok_or_else(|| "this phone's key is not one of the vault's signers".to_owned()),
    ));
    let calls: Vec<Call> = request.map(|r| r.calls).unwrap_or_default();
    let (kind, rotation) = classify(review, vault.as_ref(), &calls, fee_payers, &mut checks);
    let violations = policy.violations(&calls, fee_payers);
    checks.push(check(
        "This phone's policy allows it",
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations.join("; "))
        },
    ));
    Analysis {
        kind,
        checks,
        calls: calls
            .iter()
            .map(|c| call_view(c, review, policy, fee_payers))
            .collect(),
        vault,
        chain,
        rotation,
    }
}

/// Classifies the calls and checks a rotation or swap.
fn classify(
    review: &DeviceReview,
    vault: Option<&Vault>,
    calls: &[Call],
    fee_payers: &[WireField],
    checks: &mut Vec<Check>,
) -> (ReviewKind, Option<Rotation>) {
    let Some(vault) = vault else {
        return (ReviewKind::Payment, None);
    };
    let rotations: Vec<&Call> = calls
        .iter()
        .filter(|c| is_rotation_call(c, vault))
        .collect();
    let Some(first) = rotations.first() else {
        let kind = if calls.iter().all(|c| fee_payers.contains(&c.to)) {
            ReviewKind::Cancellation
        } else {
            ReviewKind::Payment
        };
        return (kind, None);
    };
    checks.push(check(
        "Besides one rotation, it only pays a trusted fee payer",
        if rotations.len() == 1
            && calls
                .iter()
                .all(|c| is_rotation_call(c, vault) || fee_payers.contains(&c.to))
        {
            Ok(())
        } else {
            Err("a rotation must not carry other calls".to_owned())
        },
    ));
    let proposed = match Rotation::from_call(first, vault) {
        Ok(rotation) => rotation,
        Err(error) => {
            checks.push(check("The rotation decodes", Err(error.to_string())));
            return (ReviewKind::Rotation, None);
        }
    };
    if signer_swap_slot(vault, &proposed).is_ok() {
        checks.push(check(
            "Only one signer key changes",
            check_signer_swap(vault, &proposed)
                .map(drop)
                .map_err(|e| e.to_string()),
        ));
        return (ReviewKind::SignerSwap, Some(proposed));
    }
    let reshared = review.rotation.as_ref().map(rotation);
    let result = match reshared {
        None => Err("your keeper reshared no rotation this one matches".to_owned()),
        Some(Err(error)) => Err(error.to_string()),
        Some(Ok(reshared)) if reshared != proposed => {
            Err("it performs another rotation than your keeper reshared".to_owned())
        }
        Some(Ok(_)) => check_rotation(vault, &proposed)
            .map(drop)
            .map_err(|e| e.to_string()),
    };
    checks.push(check(
        "The new committee holds the vault's key with fresh shares, as your keeper reshared",
        result,
    ));
    (ReviewKind::Rotation, Some(proposed))
}

fn call_view(
    call: &Call,
    review: &DeviceReview,
    policy: &Policy,
    fee_payers: &[WireField],
) -> CallView {
    let pinned = policy.pinned(&call.to);
    let claimed = review
        .labels
        .iter()
        .find(|l| l.address == call.to)
        .map(|l| l.label.clone());
    let decoded = call
        .signature
        .as_deref()
        .and_then(|s| arguments(s, &call.args).ok());
    let arguments = match decoded {
        Some(decoded) => decoded
            .into_iter()
            .map(|a| Argument {
                value: value(&a.kind, &a.fields),
                kind: a.kind,
            })
            .collect(),
        None => call
            .args
            .iter()
            .map(|f| Argument {
                kind: "Field".into(),
                value: display_address(f),
            })
            .collect(),
    };
    CallView {
        to: display_address(&call.to),
        label_pinned: pinned.is_some(),
        label: pinned.map(str::to_owned).or(claimed),
        function: call
            .signature
            .as_deref()
            .and_then(|s| s.split_once('('))
            .map(|(name, _)| name.to_owned()),
        arguments,
        public: call.kind == CallKind::Public,
        fee_payment: fee_payers.contains(&call.to),
    }
}

/// A parameter's display value.
fn value(kind: &str, fields: &[WireField]) -> String {
    if let [field] = fields {
        if kind == "bool" {
            return (field_u128(field) == Some(1)).to_string();
        }
        if (kind.starts_with('u') || kind.starts_with('i'))
            && let Some(n) = field_u128(field)
        {
            return n.to_string();
        }
    }
    fields
        .iter()
        .map(display_address)
        .collect::<Vec<_>>()
        .join(", ")
}
