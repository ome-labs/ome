//! The clear-signing screen: what happens if the proposal executes, the calls rebuilt on this device, what was
//! checked, and exactly what a signature covers. Nothing is signed without it.

use ome_api::daemon::{Intent, LabelSource, ProposalView, Review};

use crate::ui::{describe, format, theme};

/// A titled block of lines on the rail.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Section {
    pub(crate) title: String,
    pub(crate) lines: Vec<String>,
}

fn section(title: &str, lines: Vec<String>) -> Section {
    Section {
        title: title.to_owned(),
        lines,
    }
}

/// Whether the checks that make a signature safe all passed. A failed simulation is a warning, not a refusal: the
/// proposal may still become executable.
pub(crate) fn safe_to_sign(review: &Review) -> bool {
    let c = &review.checks;
    c.rebuilt && c.config_current && c.not_expired && c.failure.is_none()
}

/// The screen's title: `Approve proposal 4f2ac91e · Treasury · 2 of 3 to spend`.
pub(crate) fn title(verb: &str, review: &Review) -> String {
    let p = &review.proposal;
    format!(
        "{verb} proposal {} · {} · {} of {} to spend",
        format::id(&p.id),
        review.vault_name,
        p.threshold,
        p.members
    )
}

/// Every section, top to bottom. `verbose` shows full values instead of short ones.
#[expect(clippy::too_many_lines, reason = "the screen reads top to bottom")]
pub(crate) fn render(
    review: &Review,
    signing: &ome_api::daemon::Signs,
    verbose: bool,
) -> Vec<Section> {
    let p = &review.proposal;
    let full = |hex: &ome_api::Hex32| {
        if verbose {
            hex.to_string()
        } else {
            format::short(hex)
        }
    };
    let mut out = vec![section(
        "What happens if it executes",
        effect(review, &full),
    )];

    let calls = p
        .calls
        .iter()
        .enumerate()
        .flat_map(|(i, call)| {
            let target = describe::target(call, &|_| None);
            let kind = if call.is_static {
                theme::dim(" (static)").to_string()
            } else {
                String::new()
            };
            let mut lines = vec![format!(
                "{}  {target} · {}{kind}",
                i + 1,
                theme::name(&call.function)
            )];
            lines.extend(
                format::args(&call.args, verbose, 8)
                    .into_iter()
                    .map(|arg| format!("     {}", theme::dim(arg))),
            );
            lines
        })
        .collect();
    out.push(section("The calls you sign, rebuilt on this device", calls));

    let c = &review.checks;
    let mut checks = vec![
        format!(
            "{} the calls rebuild from the proposal and hash to its outer hash  {}",
            theme::check(c.rebuilt),
            theme::dim(full(&c.outer_hash))
        ),
        format!(
            "{} {}",
            theme::check(c.config_current),
            if c.config_current {
                format!(
                    "the config is current (version {})",
                    p.current_config_version
                )
            } else {
                format!(
                    "made under config version {}; the vault is at {}",
                    p.config_version, p.current_config_version
                )
            }
        ),
        format!(
            "{} the deadline is {}",
            theme::check(c.not_expired),
            format::relative(p.expires_at)
        ),
    ];
    let sim = &review.simulation;
    checks.push(match (&sim.gas, &sim.error) {
        (Some(gas), _) if sim.ok => format!(
            "{} it simulates: {} L2 gas, {} DA gas",
            theme::check(true),
            format::group_thousands(&gas.l2.to_string()),
            format::group_thousands(&gas.da.to_string())
        ),
        _ if sim.ok => format!("{} it simulates", theme::check(true)),
        (_, error) => format!(
            "{} it does not simulate yet: {}",
            theme::caution(),
            error.as_deref().unwrap_or("no reason given")
        ),
    });
    if let Some(recipient) = &review.recipient {
        let name = recipient.label.clone().unwrap_or_default();
        match recipient.source {
            Some(LabelSource::AztecWallet) => checks.push(format!(
                "{} {name} is your aztec-wallet alias, a name you set that the vault never verified",
                theme::caution()
            )),
            Some(LabelSource::Member) => {}
            None => checks.push(format!(
                "{} the recipient has no name in your aztec-wallet aliases: check the address",
                theme::caution()
            )),
        }
    }
    if let Some(failure) = &c.failure {
        checks.push(format!("{} {failure}", theme::check(false)));
    }
    out.push(section("Checks", checks));

    out.push(section("Votes", votes(p)));

    out.push(section(
        "You sign",
        vec![format!(
            "{}  =  {}",
            signing.formula,
            theme::bold(full(&signing.message))
        )],
    ));
    out
}

fn effect(review: &Review, full: &dyn Fn(&ome_api::Hex32) -> String) -> Vec<String> {
    let p = &review.proposal;
    let mut lines = Vec::new();
    match &p.intent {
        Intent::Transfer { to, amount, .. } => {
            let who = match &review.recipient {
                Some(r) => match (&r.label, r.source) {
                    (Some(label), Some(LabelSource::AztecWallet)) => {
                        format!("{label} (accounts:{label} in aztec-wallet)")
                    }
                    (Some(label), Some(LabelSource::Member)) => format!("{label} (a member)"),
                    (Some(label), None) => label.clone(),
                    (None, _) => "an address you have not named".into(),
                },
                None => "an address you have not named".into(),
            };
            let base = p
                .token
                .as_ref()
                .map(|_| format!("  {}", theme::dim(format!("({} base units)", amount.0))))
                .unwrap_or_default();
            lines.push(format!(
                "Pay {} to {who}  {}{base}",
                theme::bold(describe::amount(amount.0, p.token.as_ref())),
                theme::dim(full(to))
            ));
        }
        Intent::Cancel { cancels, .. } => lines.push(format!(
            "Cancel proposal {}: its nonce is spent, so it can never execute",
            format::id(cancels)
        )),
        Intent::Calls { .. } => lines.push(capitalize(&describe::intent(p))),
    }
    if let Some(memo) = describe::memo(p) {
        lines.push(format!(
            "Memo from {}: {}",
            p.proposer.name,
            theme::name(format!("{memo:?}"))
        ));
    } else {
        lines.push(format!("Proposed by {}", p.proposer.name));
    }
    lines.push(describe::fee(&p.fee_method));
    lines
}

fn votes(p: &ProposalView) -> Vec<String> {
    let mut lines = vec![describe::votes(p)];
    for vote in &p.approvals {
        lines.push(format!(
            "{} {} approved{}",
            theme::check(vote.valid),
            vote.name,
            if vote.valid {
                ""
            } else {
                " (signature does not verify)"
            }
        ));
    }
    for vote in &p.rejections {
        let reason = vote
            .reason
            .as_ref()
            .map(|r| format!(": {r:?}"))
            .unwrap_or_default();
        lines.push(format!(
            "{} {} rejected{reason}",
            theme::bad("✗"),
            vote.name
        ));
    }
    lines
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The sections as plain text, as the rail shows them.
#[cfg(test)]
fn to_text(sections: &[Section]) -> String {
    sections
        .iter()
        .map(|s| {
            let mut text = format!("◇  {}", s.title);
            for line in &s.lines {
                text.push_str("\n│  ");
                text.push_str(line);
            }
            text
        })
        .collect::<Vec<_>>()
        .join("\n│\n")
}

#[cfg(test)]
mod tests {
    use crate::fixtures;

    use super::*;

    #[test]
    fn the_approval_screen() {
        console::set_colors_enabled(false);
        let review = fixtures::review();
        assert!(safe_to_sign(&review));
        assert_eq!(
            title("Approve", &review),
            "Approve proposal 4f2ac91e · Treasury · 2 of 3 to spend"
        );
        insta::assert_snapshot!(to_text(&render(&review, &review.signs, false)));
    }

    #[test]
    fn a_failed_check_is_not_safe_to_sign() {
        let mut review = fixtures::review();
        review.checks.rebuilt = false;
        review.checks.failure = Some("the payload does not match the intent".into());
        assert!(!safe_to_sign(&review));
        let text = to_text(&render(&review, &review.signs, false));
        assert!(text.contains("✗ the payload does not match the intent"));
    }
}
