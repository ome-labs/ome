//! Words for what the daemon reports: a proposal in one line, its status and votes, a network's name.

use std::fmt::Write;

use ome_api::{
    Amount, Hex32,
    daemon::{
        BalanceChange, BalanceState, DaemonEvent, DecodedAuthwit, Effects, ExecutionPhase,
        FeeEstimate, FeePayerKind, FeesStatus, GovernanceChange, Intent, PolicyWarning,
        ProposalStatus, ProposalView, SimulationError, SimulationFailure, TokenInfo,
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

/// The authwits a proposal grants, one sentence each with the call's arguments beneath: what a contract may do with
/// the vault's assets inside this transaction.
pub(crate) fn authwits(
    authwits: &[DecodedAuthwit],
    labels: Labels<'_>,
    verbose: bool,
) -> Vec<String> {
    authwits
        .iter()
        .flat_map(|a| {
            let caller = a
                .caller_label
                .clone()
                .or_else(|| labels(&a.caller))
                .unwrap_or_else(|| format::short(&a.caller));
            let mut lines = vec![format!(
                "{caller} may call {} on {} for the vault, once, in this transaction",
                theme::name(&a.call.function),
                target(&a.call, labels)
            )];
            lines.extend(
                format::limited(crate::ui::abi::decoded(&a.call.args, verbose), verbose, 8)
                    .into_iter()
                    .map(|arg| format!("     {}", theme::dim(arg))),
            );
            lines
        })
        .collect()
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
    let approvals = p.approvals().filter(|v| v.valid).count();
    let mut text = format!("{approvals} of {} approvals", p.threshold);
    let rejected = p.rejections().count();
    if rejected > 0 {
        let _ = write!(text, ", {rejected} rejected");
    }
    text
}

/// The status as a word and its tone.
pub(crate) fn status(p: &ProposalView) -> (String, Tone) {
    match p.status {
        ProposalStatus::Collecting => (
            format!(
                "{}/{}",
                p.approvals().filter(|v| v.valid).count(),
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

/// Who executed a proposal, or who is executing it now: `executed by Alice 5m ago in block 1,234`.
pub(crate) fn execution(p: &ProposalView) -> Option<String> {
    if let Some(e) = &p.execution {
        let by = e
            .executor
            .as_ref()
            .map_or_else(String::new, |m| format!(" by {}", m.name));
        let block = e
            .block
            .map(|b| format!(" in block {}", format::group_thousands(&b.to_string())))
            .unwrap_or_default();
        return Some(format!("executed{by} {}{block}", format::relative(e.at)));
    }
    p.executing.as_ref().map(|x| {
        let who = if x.mine {
            "you are".to_owned()
        } else {
            format!("{} is", x.member.name)
        };
        format!(
            "{} {who} executing it: {} · started {}",
            theme::caution(),
            format!("{:?}", x.phase).to_lowercase(),
            format::relative(x.since)
        )
    })
}

/// Every vote in the order this member's keeper stored them, with when.
pub(crate) fn vote_timeline(p: &ProposalView) -> Vec<String> {
    let mut votes: Vec<_> = p.votes.iter().collect();
    votes.sort_by_key(|v| v.at);
    votes
        .into_iter()
        .map(|v| {
            let (mark, verb) = if v.approve {
                (theme::check(v.valid).to_string(), "approved")
            } else {
                (theme::bad("✗").to_string(), "rejected")
            };
            let reason = v
                .reason
                .as_ref()
                .map(|r| format!(": {r:?}"))
                .unwrap_or_default();
            let invalid = if v.valid {
                ""
            } else {
                " (signature does not verify)"
            };
            format!(
                "{mark} {} {verb}{reason}{invalid}  {}",
                v.name,
                theme::dim(format::relative(v.at))
            )
        })
        .collect()
}

/// What a transaction is expected to cost: `fee about 0.0012 Fee Juice, at most 0.0018`.
pub(crate) fn fee_estimate(estimate: &FeeEstimate) -> String {
    let units = |amount: &Amount| format::amount(amount.0, FEE_JUICE_DECIMALS);
    format!(
        "fee about {} Fee Juice, at most {}, paid by {}",
        units(&estimate.fee),
        units(&estimate.max_fee),
        format::short(&estimate.payer)
    )
}

/// The fee payer in one line: `✓ fees     sponsored 0x2d8f…69d3 · 1,000 Fee Juice`.
pub(crate) fn fees(fees: &FeesStatus) -> String {
    let kind = match fees.kind {
        FeePayerKind::Sponsored => "sponsored",
        FeePayerKind::Own => "your own",
    };
    let juice = fees.fee_juice.map_or_else(
        || "balance unknown".to_owned(),
        |j| format!("{} Fee Juice", format::amount(j.0, FEE_JUICE_DECIMALS)),
    );
    let mark = if fees.healthy {
        theme::check(true).to_string()
    } else {
        theme::check(false).to_string()
    };
    format!(
        "{mark} fees     {kind} {} · {juice}",
        format::short(&fees.payer)
    )
}

/// A line for a lifecycle event worth telling a member who is watching: another member executing, a proposal about to
/// expire, one that expired or was cancelled.
pub(crate) fn lifecycle_notice(event: &DaemonEvent) -> Option<String> {
    match event {
        DaemonEvent::ProposalExecuting {
            proposal,
            summary,
            by,
            phase,
            ..
        } if !matches!(phase, ExecutionPhase::Mined | ExecutionPhase::Failed) => Some(format!(
            "{} is executing {} ({}): {summary}",
            by.name,
            format::id(proposal),
            format!("{phase:?}").to_lowercase()
        )),
        DaemonEvent::ProposalExpiring {
            proposal,
            summary,
            at,
            ..
        } => Some(format!(
            "{} expires {}: {summary}",
            format::id(proposal),
            format::relative(*at)
        )),
        DaemonEvent::ProposalExpired {
            proposal, summary, ..
        } => Some(format!("{} expired: {summary}", format::id(proposal))),
        DaemonEvent::ProposalCancelled {
            proposal, summary, ..
        } => Some(format!("{} was cancelled: {summary}", format::id(proposal))),
        DaemonEvent::VaultRenamed { name, by, .. } => {
            Some(format!("{} renamed the vault to {name}", by.name))
        }
        DaemonEvent::MemberRenamed { member, .. } => Some(format!(
            "member {} is now called {}",
            member.index, member.name
        )),
        _ => None,
    }
}

/// A balance as a table cell: the amount, how many keepers answered, or why it failed.
pub(crate) fn balance_state(state: &BalanceState, decimals: u8) -> (String, Tone) {
    match state {
        BalanceState::Known { amount } => (format::amount(amount.0, decimals), Tone::Bold),
        BalanceState::Unreadable { answered, needed } => (
            format!("unreadable: {answered} of {needed} keepers answered"),
            Tone::Warn,
        ),
        BalanceState::Failed { message } => (format!("can't read: {message}"), Tone::Dim),
    }
}

/// Fee Juice, like Ether, has 18 decimals.
pub(crate) const FEE_JUICE_DECIMALS: u8 = 18;

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
        SimulationFailure::UnusedAuthwit { index, contract } => format!(
            "it grants authwit {} for {}, which the transaction never uses",
            index + 1,
            format::short(contract)
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
        let unused = SimulationError {
            failure: SimulationFailure::UnusedAuthwit {
                index: 0,
                contract: Hex32([7; 32]),
            },
            message: String::new(),
        };
        assert!(simulation_failure(&unused).starts_with("it grants authwit 1 for 0x"));
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
    fn executions_name_who_ran_them_and_who_is_running_them() {
        console::set_colors_enabled(false);
        let mut p = crate::fixtures::proposal();
        let alice = ome_api::daemon::MemberRef {
            index: 1,
            name: "Alice".into(),
        };
        p.executing = Some(ome_api::daemon::ExecutionView {
            member: alice.clone(),
            phase: ExecutionPhase::Proving,
            since: format::now() - 40,
            mine: false,
        });
        assert_eq!(
            execution(&p).unwrap(),
            "▲ Alice is executing it: proving · started 40s ago"
        );
        p.execution = Some(ome_api::daemon::Execution {
            tx_hash: Hex32([1; 32]),
            at: format::now() - 300,
            executor: Some(alice),
            block: Some(1234),
        });
        assert_eq!(
            execution(&p).unwrap(),
            "executed by Alice 5m ago in block 1,234"
        );
        let notice = lifecycle_notice(&DaemonEvent::ProposalExpired {
            vault: Hex32([2; 32]),
            proposal: p.id,
            summary: "Pay 250 USDC to bob".into(),
        });
        assert!(notice.unwrap().ends_with("expired: Pay 250 USDC to bob"));
    }

    #[test]
    fn networks_have_names() {
        assert_eq!(network(Some(31_337), Some(1)), "local network");
        assert_eq!(network_id("aztec:11155111:679261766"), "Testnet");
        assert_eq!(network_id("aztec:5:7"), "chain 5, rollup 7");
        assert_eq!(network_id("aztec:?:?"), "aztec:?:?");
    }
}
