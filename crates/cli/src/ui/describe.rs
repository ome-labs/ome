//! Words for what the daemon reports: a proposal in one line, its status and votes, a network's name.

use std::fmt::Write;

use ome_api::{
    Amount, Hex32,
    daemon::{
        BalanceChange, Effects, GovernanceChange, Intent, PolicyWarning, ProposalStatus,
        ProposalView, SimulationError, SimulationFailure, TokenInfo,
    },
};

use crate::ui::{format, table::Tone, theme};

/// Labels for addresses, from the address book, vaults and aliases.
pub(crate) type Labels<'a> = &'a dyn Fn(&Hex32) -> Option<String>;

fn no_labels(_: &Hex32) -> Option<String> {
    None
}

/// `250 USDC`, or base units when the token is unknown.
pub(crate) fn amount(base_units: u128, token: Option<&TokenInfo>) -> String {
    match token {
        Some(t) => format!("{} {}", format::amount(base_units, t.decimals), t.symbol),
        None => format!(
            "{} base units",
            format::group_thousands(&base_units.to_string())
        ),
    }
}

/// What the proposal does, in one line: `pay 250 USDC → bob`.
pub(crate) fn intent(p: &ProposalView) -> String {
    intent_with(p, &no_labels)
}

pub(crate) fn intent_with(p: &ProposalView, labels: Labels<'_>) -> String {
    let name = |address: &Hex32| labels(address).unwrap_or_else(|| format::short(address));
    match &p.intent {
        Intent::Transfer { to, amount, .. } => {
            format!(
                "pay {} → {}",
                self::amount(amount.0, p.token.as_ref()),
                name(to)
            )
        }
        Intent::Cancel { cancels, .. } => format!("cancel {}", format::id(cancels)),
        Intent::SetThreshold { threshold, .. } => format!("need {threshold} approvals to spend"),
        Intent::InvalidatePending { .. } => "void every approval collected so far".to_owned(),
        Intent::Calls { calls, .. } => match calls.as_slice() {
            [call] if call.to == p.vault && call.function == "rotate_membership" => {
                "change membership".to_owned()
            }
            [call] if call.to == p.vault => format!("{} on this vault", call.function),
            [call] => format!("call {} on {}", call.function, name(&call.to)),
            calls => format!(
                "{} calls: {}",
                calls.len(),
                calls
                    .iter()
                    .map(|c| c.function.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
    }
}

/// A call's target: its label, `SponsoredFPC` for the fee payer, or the short address.
pub(crate) fn target(call: &ome_api::daemon::DecodedCall, labels: Labels<'_>) -> String {
    call.to_label
        .clone()
        .or_else(|| labels(&call.to))
        .or_else(|| (call.function == "sponsor_unconditionally").then(|| "SponsoredFPC".to_owned()))
        .unwrap_or_else(|| format::short(&call.to))
}

/// Who pays the fee, as a sentence. The vault never pays in public; `external` is a sponsored fee payer.
pub(crate) fn fee(method: &str) -> String {
    match method {
        "external" => {
            "The fee is paid by the sponsored fee payer, never by the vault in public".into()
        }
        other => format!("The fee is paid by {other}, never by the vault in public"),
    }
}

/// The memo, if any.
pub(crate) fn memo(p: &ProposalView) -> Option<&str> {
    match &p.intent {
        Intent::Transfer { memo, .. }
        | Intent::Calls { memo, .. }
        | Intent::Cancel { memo, .. }
        | Intent::SetThreshold { memo, .. }
        | Intent::InvalidatePending { memo } => memo.as_deref(),
    }
}

/// `1 of 2 approvals`, with rejections when there are any.
pub(crate) fn votes(p: &ProposalView) -> String {
    let approvals = p.approvals.iter().filter(|v| v.valid).count();
    let mut text = format!("{approvals} of {} approvals", p.threshold);
    if !p.rejections.is_empty() {
        let _ = write!(text, ", {} rejected", p.rejections.len());
    }
    text
}

/// The status as a word and its tone.
pub(crate) fn status(p: &ProposalView) -> (String, Tone) {
    match p.status {
        ProposalStatus::Collecting => (
            format!(
                "{}/{}",
                p.approvals.iter().filter(|v| v.valid).count(),
                p.threshold
            ),
            Tone::Warn,
        ),
        ProposalStatus::Ready => ("ready".into(), Tone::Ok),
        ProposalStatus::Blocked => ("blocked".into(), Tone::Bad),
        ProposalStatus::Executed => ("executed".into(), Tone::Dim),
        ProposalStatus::Cancelled => ("cancelled".into(), Tone::Dim),
        ProposalStatus::Expired => ("expired".into(), Tone::Dim),
        ProposalStatus::Stale => ("stale".into(), Tone::Dim),
    }
}

/// Whether the proposal can still change: collecting or ready.
pub(crate) const fn is_open(status: ProposalStatus) -> bool {
    matches!(status, ProposalStatus::Collecting | ProposalStatus::Ready)
}

/// What this member can still do with it.
pub(crate) fn can_vote(p: &ProposalView) -> bool {
    is_open(p.status) && !p.approved_by_me && !p.rejected_by_me
}

/// A network by its L1 chain and rollup version: `Testnet`, `local network`, or the ids.
pub(crate) fn network(l1: Option<u64>, rollup: Option<u64>) -> String {
    match (l1, rollup) {
        (Some(31_337), _) => "local network".into(),
        (Some(11_155_111), Some(679_261_766)) => "Testnet".into(),
        (Some(1), _) => "Alpha".into(),
        (Some(l1), Some(rollup)) => format!("chain {l1}, rollup {rollup}"),
        _ => "unknown network".into(),
    }
}

/// A network as an invite names it (`aztec:<l1>:<rollup>`).
pub(crate) fn network_id(id: &str) -> String {
    let mut parts = id.strip_prefix("aztec:").unwrap_or(id).split(':');
    let l1 = parts.next().and_then(|p| p.parse().ok());
    let rollup = parts.next().and_then(|p| p.parse().ok());
    if l1.is_none() {
        return id.to_owned();
    }
    network(l1, rollup)
}

/// "2 of 3 to spend · 2 of 3 to read".
pub(crate) fn shape(threshold: u32, viewing_threshold: u32, members: u32) -> String {
    format!("{threshold} of {members} to spend · {viewing_threshold} of {members} to read")
}

/// A risk in a vault's thresholds, in words, and whether it can strand the funds for good.
pub(crate) struct Risk {
    pub(crate) severe: bool,
    pub(crate) text: String,
}

/// The risks of `k` to spend and `t_v` to read among `n` members (`ome-protocol`'s policy warnings).
pub(crate) fn risks(threshold: u32, viewing_threshold: u32, members: u32) -> Vec<Risk> {
    use ome_protocol::membership::policy::{Warning, warnings};
    warnings(threshold, viewing_threshold, members)
        .into_iter()
        .map(|warning| match warning {
            Warning::SingleSigner => PolicyWarning::SingleSigner,
            Warning::NoSpareSigner => PolicyWarning::NoSpareSigner,
            Warning::TwoCanRead => PolicyWarning::TwoCanRead,
            Warning::NoSpareViewer => PolicyWarning::NoSpareViewer,
        })
        .map(|warning| risk(warning, members))
        .collect()
}

/// A policy warning in words: what the shape risks among `members`.
pub(crate) fn risk(warning: PolicyWarning, members: u32) -> Risk {
    match warning {
            PolicyWarning::SingleSigner => Risk {
                severe: false,
                text: "any one member can spend alone".into(),
            },
            PolicyWarning::NoSpareSigner => Risk {
                severe: true,
                text: format!(
                    "all {members} members must approve: one lost signing key stops all spending for good"
                ),
            },
            PolicyWarning::TwoCanRead => Risk {
                severe: false,
                text: "any 2 members, even 2 who later leave, can read what arrived while they were members"
                    .into(),
            },
            PolicyWarning::NoSpareViewer => Risk {
                severe: true,
                text: format!(
                    "all {members} keepers are needed to read: lose one and the vault can never find its notes \
                     again, so its funds are stuck"
                ),
            },
    }
}

/// A balance change as people read it: `The vault's OMET: 1,000 → 924.5`.
pub(crate) fn balance_change(change: &BalanceChange) -> String {
    let units = |amount: &Amount| format::amount(amount.0, change.token.decimals);
    format!(
        "The vault's {}: {} → {}",
        change.token.symbol,
        units(&change.before),
        units(&change.after)
    )
}

/// What a simulation shows the transaction does to the vault: balances before and after, and what it can't value.
pub(crate) fn effects(effects: Option<&Effects>) -> Vec<String> {
    let Some(effects) = effects else {
        return Vec::new();
    };
    let mut lines: Vec<String> = effects.balances.iter().map(balance_change).collect();
    if !effects.complete {
        lines.push(format!(
            "{} it may change other balances: some of the vault's notes it touches aren't a tracked token's",
            theme::caution()
        ));
    }
    lines
}

/// A change to the vault's policy, with the risks of the new shape.
pub(crate) fn governance(change: &GovernanceChange) -> Vec<String> {
    match change {
        GovernanceChange::Threshold {
            before,
            after,
            members,
            warnings,
        } => {
            let mut lines = vec![format!(
                "Approvals to spend: {before} of {members} → {} of {members}",
                theme::bold(after.to_string())
            )];
            lines.extend(warnings.iter().map(|w| {
                let risk = risk(*w, *members);
                let mark = if risk.severe {
                    theme::bad("✗").to_string()
                } else {
                    theme::caution().to_string()
                };
                format!("{mark} {}", risk.text)
            }));
            lines.push("Open proposals made under the old config need their votes again".into());
            lines
        }
        GovernanceChange::InvalidatePending { version } => vec![format!(
            "Every approval collected under config version {version} stops counting: open proposals need their \
             votes again"
        )],
    }
}

/// Why a simulation failed, in words.
pub(crate) fn simulation_failure(error: &SimulationError) -> String {
    match &error.failure {
        SimulationFailure::InsufficientBalance { token, contract } => format!(
            "the vault holds too little {}",
            token
                .as_ref()
                .map_or_else(|| format::short(contract), |t| t.symbol.clone())
        ),
        SimulationFailure::AuthwitRequired { contract } => format!(
            "{} needs an authwit from the vault, which this proposal doesn't carry",
            contract
                .as_ref()
                .map_or_else(|| "a contract".to_owned(), format::short)
        ),
        SimulationFailure::NonceUsed => "it was already executed or cancelled".to_owned(),
        SimulationFailure::Reverted { function, .. } => match function {
            Some(function) => format!("{function} reverted: {}", error.message),
            None => error.message.clone(),
        },
        SimulationFailure::Unavailable => format!("it can't be simulated now: {}", error.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risks_name_the_shapes_that_strand_funds() {
        let severe = |k, t, n| risks(k, t, n).iter().filter(|r| r.severe).count();
        assert_eq!(severe(2, 2, 3), 0);
        // A 2-of-2 vault needs both keepers to read and both keys to spend.
        assert_eq!(severe(2, 2, 2), 2);
        assert_eq!(severe(2, 3, 3), 1);
        assert_eq!(risks(2, 3, 4).len(), 0);
    }

    #[test]
    fn simulation_failures_and_threshold_changes_read_as_words() {
        console::set_colors_enabled(false);
        let short = SimulationError {
            failure: SimulationFailure::InsufficientBalance {
                token: None,
                contract: Hex32([7; 32]),
            },
            message: String::new(),
        };
        assert!(simulation_failure(&short).starts_with("the vault holds too little 0x"));
        let lines = governance(&GovernanceChange::Threshold {
            before: 2,
            after: 3,
            members: 3,
            warnings: vec![PolicyWarning::NoSpareSigner],
        });
        assert_eq!(lines[0], "Approvals to spend: 2 of 3 → 3 of 3");
        assert!(lines[1].starts_with("✗ all 3 members must approve"));
    }

    #[test]
    fn networks_have_names() {
        assert_eq!(network(Some(31_337), Some(1)), "local network");
        assert_eq!(network_id("aztec:11155111:679261766"), "Testnet");
        assert_eq!(network_id("aztec:5:7"), "chain 5, rollup 7");
        assert_eq!(network_id("aztec:?:?"), "aztec:?:?");
    }
}
