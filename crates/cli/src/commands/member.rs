//! `ome member request|change|join|status|swap-key`. A change of members reshares the viewing key to the new
//! committee, then one `rotate_membership` proposal changes signers, thresholds and committee together. The reshare
//! and the rotation are steps here, not commands.

use std::time::Duration;

use ome_api::{
    Empty,
    daemon::{
        MembershipBegin, MembershipGet, MembershipList, MembershipParams, MembershipPlan,
        MembershipPlanGet, MembershipRequest, MembershipSwapSigner, ProposalRef, ProposalStatus,
        ProposalsGet, RotationPlanText, RotationRef, RotationState, RotationStepId, StepStatus,
        SwapSignerParams, VaultDetailsGet, VaultSummary,
    },
    keeper::VaultParams,
};

use crate::{
    args::{ChangeArgs, MemberCommand, ProposalArg},
    commands::proposal,
    context::{Ctx, Mode},
    error::{Error, Result},
    refs,
    ui::{
        format, prompt,
        steps::{Step, Tracked, Tracker},
        table::{self, Tone},
        theme,
    },
};

const POLL: Duration = Duration::from_secs(1);

pub(crate) async fn run(ctx: &Ctx, command: MemberCommand) -> Result<()> {
    match command {
        MemberCommand::Request => request(ctx).await,
        MemberCommand::Change(args) => change(ctx, args).await,
        MemberCommand::Join { plan } => join(ctx, plan).await,
        MemberCommand::Status { change } => status(ctx, change).await,
        MemberCommand::SwapKey { member, key } => swap_key(ctx, member, key).await,
    }
}

async fn request(ctx: &Ctx) -> Result<()> {
    let request = ctx
        .client()
        .await?
        .call::<MembershipRequest>(&Empty {})
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&request);
    }
    if !ctx.human() {
        ctx.out(&request.request);
        return Ok(());
    }
    ctx.section(
        "Your join request: send this line to a member of the vault",
        &[format!(
            "They run {}",
            theme::name("ome member change --add <request>")
        )],
    );
    ctx.out(&request.request);
    Ok(())
}

#[expect(clippy::too_many_lines, reason = "a guided flow reads top to bottom")]
async fn change(ctx: &Ctx, args: ChangeArgs) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let client = ctx.client().await?;
    let details = client
        .call::<VaultDetailsGet>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    let members = &details.committee.members;
    ctx.intro(&format!("Change the members of {}", vault.name));
    let named = !args.add.is_empty() || !args.remove.is_empty();
    let mut add = args.add;
    if add.is_empty() && args.remove.is_empty() && ctx.interactive {
        loop {
            let request: String =
                cliclack::input("A newcomer's join request (from `ome member request`)")
                    .placeholder("Enter when there is no one (more) to add")
                    .required(false)
                    .validate(|v: &String| {
                        if v.is_empty() || v.starts_with("ome-join-v0:") {
                            Ok(())
                        } else {
                            Err("A join request starts with ome-join-v0:")
                        }
                    })
                    .interact()?;
            if request.is_empty() {
                break;
            }
            add.push(request);
        }
    }
    let mut remove: Vec<u32> = Vec::new();
    for input in &args.remove {
        remove.push(refs::member(ctx, members, input)?.index);
    }
    if !named && ctx.interactive {
        let mut select =
            cliclack::multiselect("Who leaves? (space to pick, Enter when done)").required(false);
        for m in members {
            select = select.item(m.index, &m.name, if m.you { "you" } else { "" });
        }
        remove = select.interact()?;
    }
    if add.is_empty() && remove.is_empty() {
        return Err(
            Error::usage("Nothing to change: add someone, remove someone, or both.")
                .hint("Pass --add <request> or --remove <member>."),
        );
    }
    let size = u32::try_from(members.len()).unwrap_or(0) - u32::try_from(remove.len()).unwrap_or(0)
        + u32::try_from(add.len()).unwrap_or(0);
    if size < 2 {
        return Err(Error::usage("A vault keeps at least 2 members."));
    }
    let approvals = prompt::number(
        ctx,
        args.approvals,
        "--approvals",
        "Approvals needed to spend afterwards (k)",
        vault.threshold.min(size),
        1..=size,
    )?;
    let readers = prompt::number(
        ctx,
        args.readers,
        "--readers",
        "Members needed to read afterwards (t_v)",
        vault.viewing_threshold.clamp(2, size),
        2..=size,
    )?;
    let step = Step::start(ctx, "Checking the plan");
    let plan = client
        .call::<MembershipPlanGet>(&MembershipParams {
            vault: vault.address,
            remove: remove.clone(),
            add,
            threshold: approvals,
            viewing_threshold: readers,
        })
        .await;
    let plan = match plan {
        Ok(plan) => {
            step.clear();
            plan
        }
        Err(error) => {
            step.fail("The plan was refused");
            return Err(error.into());
        }
    };
    show_plan(ctx, &plan);
    if let Some(failure) = &plan.failure {
        return Err(Error::usage(format!("This change cannot run: {failure}")));
    }
    crate::commands::vault::accept_risks(
        ctx,
        plan.next.threshold,
        plan.next.viewing_threshold,
        u32::try_from(plan.next.members.len()).unwrap_or(u32::MAX),
    )?;
    if remove.is_empty() {
        prompt::proceed(ctx, "Start this change?")?;
    } else {
        prompt::typed(
            ctx,
            &vault.name,
            args.confirm.as_deref(),
            "Removing members",
        )?;
    }
    let state = client
        .call::<MembershipBegin>(&RotationPlanText {
            plan: plan.share.clone(),
        })
        .await?;
    share_plan(ctx, &plan.share);
    follow(ctx, &vault, state, true).await
}

fn show_plan(ctx: &Ctx, plan: &MembershipPlan) {
    let shape = |s: &ome_api::daemon::CommitteeShape| {
        format!(
            "{} · {} of {} to spend · {} to read",
            s.members.join(", "),
            s.threshold,
            s.members.len(),
            s.viewing_threshold
        )
    };
    let leaving: Vec<&String> = plan
        .current
        .members
        .iter()
        .filter(|m| !plan.next.members.contains(m))
        .collect();
    let mut lines = vec![
        format!("now    {}", shape(&plan.current)),
        format!("after  {}", theme::bold(shape(&plan.next))),
    ];
    for j in &plan.joiners {
        lines.push(format!(
            "{} {} joins  {}",
            theme::ok("+"),
            j.name,
            theme::dim(format::short_point(&j.signer_key))
        ));
    }
    for name in &leaving {
        lines.push(format!("{} {name} leaves", theme::bad("−")));
    }
    ctx.section("The change", &lines);
    let mut checks = vec![
        format!(
            "{} the members who stay reshare the viewing key: it stays the vault's key, and every share is fresh",
            theme::check(true)
        ),
        format!(
            "{} it removes at most {} member(s) at once (t_v − 1)",
            theme::check(plan.within_removal_limit),
            plan.current.viewing_threshold.saturating_sub(1)
        ),
    ];
    if !leaving.is_empty() {
        checks.push(format!(
            "{} those who leave keep what they saw before, including who paid the vault (`ome counterparties`)",
            theme::caution()
        ));
    }
    if let Some(failure) = &plan.failure {
        checks.push(format!("{} {failure}", theme::check(false)));
    }
    ctx.section("Checks", &checks);
}

fn share_plan(ctx: &Ctx, share: &str) {
    if !ctx.human() {
        eprintln!("plan: {share}");
        return;
    }
    ctx.section(
        "Send this plan to every member who stays and every newcomer",
        &[format!(
            "Each runs {}",
            theme::name("ome member join <plan>")
        )],
    );
    // On its own line, unindented, so it copies whole however the terminal wraps it.
    eprintln!("{}\n", theme::name(share));
}

async fn join(ctx: &Ctx, plan: Option<String>) -> Result<()> {
    let plan = prompt::text(ctx, plan, "<plan>", "The plan", None, |v| {
        if v.trim().starts_with("ome-rotation-v0:") {
            Ok(())
        } else {
            Err("A plan starts with ome-rotation-v0:".into())
        }
    })?;
    let client = ctx.client().await?;
    let state = client
        .call::<MembershipBegin>(&RotationPlanText {
            plan: plan.trim().to_owned(),
        })
        .await?;
    let vaults = client.call::<ome_api::daemon::VaultList>(&Empty {}).await?;
    let vault = vaults
        .into_iter()
        .find(|v| v.address == state.plan.vault)
        .unwrap_or_else(|| VaultSummary {
            address: state.plan.vault,
            name: "the vault".into(),
            alias: None,
            is_default: false,
            threshold: state.plan.next.threshold,
            viewing_threshold: state.plan.next.viewing_threshold,
            members: u32::try_from(state.plan.next.members.len()).unwrap_or(0),
            my_index: 0,
            founded_at: 0,
        });
    ctx.intro(&format!("Take part in a change of {}", vault.name));
    show_plan(ctx, &state.plan);
    follow(ctx, &vault, state, false).await
}

fn step_label(id: RotationStepId) -> (&'static str, &'static str) {
    match id {
        RotationStepId::Reshare => ("reshare", "Reshared the viewing key to the new committee"),
        RotationStepId::Propose => ("propose", "Proposed rotate_membership"),
        RotationStepId::Approve => ("approve", "Enough members approved"),
        RotationStepId::Execute => ("execute", "Executed on chain"),
        RotationStepId::Activate => ("activate", "Keepers switched to the new shares"),
    }
}

/// Follows a rotation to its end: votes on its proposal here when it appears, and on the member who planned it,
/// executes it once it is ready.
async fn follow(
    ctx: &Ctx,
    vault: &VaultSummary,
    mut state: RotationState,
    executes: bool,
) -> Result<()> {
    let client = ctx.client().await?;
    let mut tracker = Tracker::default();
    let mut has_voted = false;
    let mut sent = false;
    loop {
        tracker.update(
            ctx,
            state.steps.iter().map(|s| {
                let (key, label) = step_label(s.id);
                Tracked {
                    key,
                    label: label.into(),
                    status: s.status,
                    detail: s.detail.clone(),
                    took: None,
                }
            }),
        );
        if state.done {
            break;
        }
        if let Some(id) = state.proposal {
            let view = client
                .call::<ProposalsGet>(&ProposalRef {
                    vault: vault.address,
                    id,
                })
                .await
                .ok();
            if let Some(view) = view {
                if !has_voted
                    && !view.approved_by_me
                    && !view.rejected_by_me
                    && view.status == ProposalStatus::Collecting
                {
                    tracker.finish();
                    let _nested = ctx.nest();
                    proposal::approve(
                        ctx,
                        ProposalArg {
                            proposal: Some(id.to_string()),
                        },
                    )
                    .await?;
                    has_voted = true;
                } else if view.approved_by_me || view.rejected_by_me {
                    has_voted = true;
                }
                if executes && !sent && view.status == ProposalStatus::Ready {
                    tracker.finish();
                    let _nested = ctx.nest();
                    proposal::execute(
                        ctx,
                        crate::args::ExecuteArgs {
                            proposal: Some(id.to_string()),
                            no_wait: false,
                            force: false,
                        },
                    )
                    .await?;
                    sent = true;
                }
            }
        }
        tokio::time::sleep(POLL).await;
        state = client
            .call::<MembershipGet>(&RotationRef {
                vault: vault.address,
                id: state.plan.id,
            })
            .await?
            .unwrap_or(state);
    }
    tracker.finish();
    if let Some(error) = &state.error {
        return Err(Error::failed(format!("The change failed: {error}")));
    }
    if ctx.mode == Mode::Json {
        return ctx.json(&state);
    }
    let next = &state.plan.next;
    ctx.outro(&format!(
        "Membership changed: {} members · {} to spend · {} to read",
        next.members.len(),
        next.threshold,
        next.viewing_threshold
    ));
    if next.members.len() < state.plan.current.members.len()
        || state
            .plan
            .current
            .members
            .iter()
            .any(|m| !next.members.contains(m))
    {
        ctx.next("counterparties");
    }
    Ok(())
}

async fn status(ctx: &Ctx, change: Option<String>) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let rotations = ctx
        .client()
        .await?
        .call::<MembershipList>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    if let Some(prefix) = change {
        let digits = prefix.trim_start_matches("0x").to_lowercase();
        let matches: Vec<_> = rotations
            .iter()
            .filter(|r| r.plan.id.to_string()[2..].starts_with(&digits))
            .collect();
        let [state] = matches.as_slice() else {
            return Err(Error::usage(format!(
                "{} changes start with {prefix:?}.",
                matches.len()
            )));
        };
        if ctx.mode == Mode::Json {
            return ctx.json(state);
        }
        show_plan(ctx, &state.plan);
        let lines: Vec<String> = state
            .steps
            .iter()
            .map(|s| {
                let mark = match s.status {
                    StepStatus::Done => theme::ok("✓").to_string(),
                    StepStatus::Failed => theme::bad("✗").to_string(),
                    StepStatus::Active => theme::warn("◒").to_string(),
                    StepStatus::Pending => theme::dim("○").to_string(),
                };
                let detail = s
                    .detail
                    .as_ref()
                    .map(|d| theme::dim(format!(" · {d}")).to_string())
                    .unwrap_or_default();
                format!("{mark} {}{detail}", step_label(s.id).1)
            })
            .collect();
        ctx.section("Steps", &lines);
        return Ok(());
    }
    if ctx.mode == Mode::Json {
        return ctx.json(&rotations);
    }
    if rotations.is_empty() {
        ctx.info(&format!("{} has had no membership changes.", vault.name));
        return Ok(());
    }
    let mut t = table::Table::new(&["change", "after", "step", "state"]);
    for r in &rotations {
        let current = r
            .steps
            .iter()
            .find(|s| s.status != StepStatus::Done)
            .map_or("done", |s| step_label(s.id).0);
        let (state, tone) = match (&r.error, r.done) {
            (Some(_), _) => ("failed", Tone::Bad),
            (None, true) => ("done", Tone::Ok),
            (None, false) => ("running", Tone::Warn),
        };
        t.row(vec![
            table::toned(format::id(&r.plan.id), Tone::Name),
            table::cell(format!(
                "{} · {} to spend · {} to read",
                r.plan.next.members.join(", "),
                r.plan.next.threshold,
                r.plan.next.viewing_threshold
            )),
            table::cell(current),
            table::toned(state, tone),
        ]);
    }
    t.print(ctx);
    Ok(())
}

async fn swap_key(ctx: &Ctx, member: Option<String>, key: Option<String>) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let client = ctx.client().await?;
    let details = client
        .call::<VaultDetailsGet>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    let members = &details.committee.members;
    let input = match member {
        Some(input) => input,
        None => prompt::select(
            ctx,
            "<member>",
            "Whose key changes?",
            members
                .iter()
                .map(|m| (m.index.to_string(), m.name.clone(), format!("#{}", m.index)))
                .collect(),
        )?,
    };
    let member = refs::member(ctx, members, &input)?;
    let key = prompt::text(
        ctx,
        key,
        "<key>",
        "The new signing key (0x<x>:0x<y>, from the new device)",
        None,
        |v| format::parse_point(v).map(|_| ()).map_err(|e| e.message),
    )?;
    let key = format::parse_point(&key)?;
    ctx.intro(&format!(
        "Swap {}'s signing key · {}",
        member.name, vault.name
    ));
    ctx.section(
        "What happens if it executes",
        &[
            format!("{}'s approvals count only from the new key", member.name),
            format!("from  {}", theme::dim(format::point(&member.signer_key))),
            format!("to    {}", theme::bold(format::point(&key))),
            theme::dim("The committee, the thresholds and the vault's address stay as they are.")
                .to_string(),
        ],
    );
    prompt::proceed(ctx, "Propose the swap?")?;
    let view = client
        .call::<MembershipSwapSigner>(&SwapSignerParams {
            vault: vault.address,
            slot: member.index - 1,
            signer_key: key,
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&view);
    }
    ctx.outro(&format!("Proposed {}", theme::name(format::id(&view.id))));
    ctx.next(&format!("approve {}", format::id(&view.id)));
    Ok(())
}
