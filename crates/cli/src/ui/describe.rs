//! Words for what the daemon reports: a proposal in one line, its status and votes, a network's name.

use std::fmt::Write;

use ome_api::{
    Hex32,
    daemon::{Intent, ProposalStatus, ProposalView, TokenInfo},
};

use crate::ui::{format, table::Tone};

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
        | Intent::Cancel { memo, .. } => memo.as_deref(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn networks_have_names() {
        assert_eq!(network(Some(31_337), Some(1)), "local network");
        assert_eq!(network_id("aztec:11155111:679261766"), "Testnet");
        assert_eq!(network_id("aztec:5:7"), "chain 5, rollup 7");
        assert_eq!(network_id("aztec:?:?"), "aztec:?:?");
    }
}
