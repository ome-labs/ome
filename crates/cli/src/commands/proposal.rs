//! `ome proposals`, `ome proposal show|review`, `ome approve`, `ome reject`, `ome execute`. Voting always shows the
//! clear-signing screen first; `--yes` skips only the final question, never the checks or the passphrase.

use ome_api::{
    Hex32,
    daemon::{
        ApproveParams, DaemonEvent, ExecutionPhase, Intent, ProposalRef, ProposalStatus,
        ProposalView, ProposalsApprove, ProposalsExecute, ProposalsList, ProposalsReject,
        ProposalsReview, RejectParams, Review, TokensList, VaultSummary,
    },
    keeper::VaultParams,
};

use crate::{
    args::{ExecuteArgs, ListArgs, ProposalArg, RejectArgs, StatusFilter},
    context::{Ctx, Mode},
    error::{Error, Result},
    refs::{self, Book},
    secret,
    ui::{
        describe, format, prompt, review,
        steps::Step,
        table::{self, Tone},
        theme,
    },
};

pub(crate) async fn list(ctx: &Ctx, args: ListArgs) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let mut proposals = ctx
        .client()
        .await?
        .call::<ProposalsList>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    proposals.retain(|p| match args.status {
        StatusFilter::Open => describe::is_open(p.status),
        StatusFilter::Ready => p.status == ProposalStatus::Ready,
        StatusFilter::Done => !describe::is_open(p.status),
        StatusFilter::All => true,
    });
    if ctx.mode == Mode::Json {
        return ctx.json(&proposals);
    }
    if proposals.is_empty() {
        ctx.info(&format!(
            "{} has no {} proposals.",
            vault.name,
            match args.status {
                StatusFilter::Open => "open",
                StatusFilter::Ready => "ready",
                StatusFilter::Done => "finished",
                StatusFilter::All => "",
            }
        ));
        ctx.next("propose transfer");
        return Ok(());
    }
    let book = Book::load(ctx).await?;
    let mut t = table::Table::new(&["id", "what", "votes", "you", "expires", "by"]);
    for p in &proposals {
        let (status, tone) = describe::status(p);
        let mine = if p.approved_by_me {
            table::toned("approved", Tone::Ok)
        } else if p.rejected_by_me {
            table::toned("rejected", Tone::Bad)
        } else if describe::can_vote(p) {
            table::toned("to vote", Tone::Warn)
        } else {
            table::cell("")
        };
        t.row(vec![
            table::toned(format::id(&p.id), Tone::Name),
            table::cell(describe::intent_with(p, &|a| book.label(a))),
            table::toned(status, tone),
            mine,
            table::toned(
                if describe::is_open(p.status) {
                    format::relative(p.expires_at)
                } else {
                    String::new()
                },
                Tone::Dim,
            ),
            table::toned(&p.proposer.name, Tone::Dim),
        ]);
    }
    t.print(ctx);
    if let Some(p) = proposals.iter().find(|p| describe::can_vote(p)) {
        ctx.next(&format!("approve {}", format::id(&p.id)));
    } else if let Some(p) = proposals.iter().find(|p| p.status == ProposalStatus::Ready) {
        ctx.next(&format!("execute {}", format::id(&p.id)));
    }
    Ok(())
}

pub(crate) async fn show(ctx: &Ctx, arg: ProposalArg) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let p = refs::proposal(ctx, vault.address, arg.proposal.as_deref(), |_| true).await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&p);
    }
    let book = Book::load(ctx).await?;
    let (status, _) = describe::status(&p);
    ctx.intro(&format!("Proposal {} · {}", format::id(&p.id), vault.name));
    let mut what = vec![describe::intent_with(&p, &|a| book.label(a))];
    if let Some(memo) = describe::memo(&p) {
        what.push(format!("memo {}", theme::name(format!("{memo:?}"))));
    }
    what.push(format!("proposed by {} · {status}", p.proposer.name));
    ctx.section("What", &what);
    let mut votes = vec![describe::votes(&p)];
    votes.extend(
        p.approvals
            .iter()
            .map(|v| format!("{} {}", theme::check(v.valid), v.name)),
    );
    votes.extend(p.rejections.iter().map(|v| {
        format!(
            "{} {}{}",
            theme::bad("✗"),
            v.name,
            v.reason
                .as_ref()
                .map(|r| format!(": {r:?}"))
                .unwrap_or_default()
        )
    }));
    ctx.section("Votes", &votes);
    let mut details = vec![
        format!("expires     {}", format::relative(p.expires_at)),
        format!(
            "config      version {} (vault at {})",
            p.config_version, p.current_config_version
        ),
        format!("fee         {}", p.fee_method),
        format!("outer hash  {}", p.outer_hash),
        format!("nonce       {}", p.tx_nonce),
    ];
    if let Some(execution) = &p.execution {
        details.push(format!(
            "executed    {} · tx {}",
            format::relative(execution.at),
            execution.tx_hash
        ));
    }
    if let Some(other) = &p.cancelled_by {
        details.push(format!("cancel      {}", format::id(other)));
    }
    ctx.section("Details", &details);
    if describe::can_vote(&p) {
        ctx.next(&format!("approve {}", format::id(&p.id)));
    } else if p.status == ProposalStatus::Ready {
        ctx.next(&format!("execute {}", format::id(&p.id)));
    }
    Ok(())
}

/// Rebuilds and checks the proposal on this device, with a spinner.
async fn fetch_review(ctx: &Ctx, vault: Hex32, id: Hex32) -> Result<Review> {
    let step = Step::start(ctx, "Rebuilding and checking it on this device");
    match ctx
        .client()
        .await?
        .call::<ProposalsReview>(&ProposalRef { vault, id })
        .await
    {
        Ok(review) => {
            step.clear();
            Ok(review)
        }
        Err(error) => {
            step.fail("Could not rebuild it");
            Err(error.into())
        }
    }
}

fn show_review(ctx: &Ctx, verb: &str, review: &Review, signing: &ome_api::daemon::Signs) {
    ctx.intro(&review::title(verb, review));
    for section in review::render(review, signing, ctx.verbose) {
        ctx.section(&section.title, &section.lines);
    }
}

pub(crate) async fn review(ctx: &Ctx, arg: ProposalArg) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let p = refs::proposal(ctx, vault.address, arg.proposal.as_deref(), |p| {
        describe::is_open(p.status)
    })
    .await?;
    let review = fetch_review(ctx, vault.address, p.id).await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&review);
    }
    show_review(ctx, "Review", &review, &review.signs);
    if review::safe_to_sign(&review) {
        ctx.outro("Every check passed");
        if describe::can_vote(&p) {
            ctx.next(&format!("approve {}", format::id(&p.id)));
        }
        Ok(())
    } else {
        Err(
            Error::failed("A check failed: do not approve it.").hint(format!(
                "Reject it with `{}`.",
                ctx.ome(&format!("reject {}", format::id(&p.id)))
            )),
        )
    }
}

pub(crate) async fn approve(ctx: &Ctx, arg: ProposalArg) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let p = refs::proposal(
        ctx,
        vault.address,
        arg.proposal.as_deref(),
        describe::can_vote,
    )
    .await?;
    let short = format::id(&p.id);
    if p.approved_by_me {
        ctx.info(&format!(
            "You approved {short} already · {}",
            describe::votes(&p)
        ));
        return Ok(());
    }
    if !describe::is_open(p.status) {
        let (status, _) = describe::status(&p);
        return Err(Error::usage(format!(
            "{short} is {status}: there is nothing to approve."
        )));
    }
    let review = fetch_review(ctx, vault.address, p.id).await?;
    if ctx.human() {
        show_review(ctx, "Approve", &review, &review.signs);
    }
    if !review::safe_to_sign(&review) {
        return Err(Error::failed(format!(
            "{} Approving it is refused.",
            review
                .checks
                .failure
                .as_deref()
                .unwrap_or("A check failed.")
        ))
        .hint(format!(
            "Reject it instead: `{}`.",
            ctx.ome(&format!("reject {short}"))
        )));
    }
    prompt::proceed(ctx, "Approve with your signing key?")?;
    let client = ctx.client().await?;
    let secret = secret::for_signing(ctx, client).await?;
    let step = Step::start(ctx, "Signing the approval");
    let approve = |secret| ApproveParams {
        vault: vault.address,
        id: p.id,
        secret,
    };
    let sent_none = secret.is_none();
    let mut view = client.call::<ProposalsApprove>(&approve(secret)).await;
    // The unlock window can close between the status check and the signature.
    if sent_none && matches!(&view, Err(e) if e.kind() == ome_api::ErrorKind::Locked) {
        step.message("The signer locked meanwhile");
        view = client
            .call::<ProposalsApprove>(&approve(Some(secret::for_unlock(ctx, false)?)))
            .await;
    }
    let view = view.map_err(|e| Error::from(e).context(format!("Can't approve {short}")));
    let view = match view {
        Ok(view) => {
            step.done("Approved");
            view
        }
        Err(error) => {
            step.fail("Not approved");
            return Err(error);
        }
    };
    voted(ctx, &view, "Approved")
}

pub(crate) async fn reject(ctx: &Ctx, args: RejectArgs) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let p = refs::proposal(
        ctx,
        vault.address,
        args.proposal.as_deref(),
        describe::can_vote,
    )
    .await?;
    let short = format::id(&p.id);
    if p.rejected_by_me {
        ctx.info(&format!("You rejected {short} already"));
        return Ok(());
    }
    let review = fetch_review(ctx, vault.address, p.id).await?;
    if ctx.human() {
        show_review(ctx, "Reject", &review, &review.rejects);
    }
    let reason = match args.reason {
        Some(reason) => Some(reason),
        None if ctx.interactive => {
            let reason: String = cliclack::input("Why? (the other members see it)")
                .placeholder("Enter to give no reason")
                .required(false)
                .interact()?;
            Some(reason).filter(|r| !r.trim().is_empty())
        }
        None => None,
    };
    if reason.as_ref().is_some_and(|r| r.chars().count() > 280) {
        return Err(Error::usage("Keep the reason to 280 characters."));
    }
    prompt::proceed(ctx, "Reject with your signing key?")?;
    let client = ctx.client().await?;
    let secret = secret::for_signing(ctx, client).await?;
    let step = Step::start(ctx, "Signing the rejection");
    let reject = |secret| RejectParams {
        vault: vault.address,
        id: p.id,
        reason: reason.clone(),
        secret,
    };
    let sent_none = secret.is_none();
    let mut view = client.call::<ProposalsReject>(&reject(secret)).await;
    if sent_none && matches!(&view, Err(e) if e.kind() == ome_api::ErrorKind::Locked) {
        step.message("The signer locked meanwhile");
        view = client
            .call::<ProposalsReject>(&reject(Some(secret::for_unlock(ctx, false)?)))
            .await;
    }
    let view = match view {
        Ok(view) => {
            step.done("Rejected");
            view
        }
        Err(error) => {
            step.fail("Not rejected");
            return Err(Error::from(error).context(format!("Can't reject {short}")));
        }
    };
    voted(ctx, &view, "Rejected")
}

fn voted(ctx: &Ctx, view: &ProposalView, verb: &str) -> Result<()> {
    if ctx.mode == Mode::Json {
        return ctx.json(view);
    }
    let short = format::id(&view.id);
    let approvals = view.approvals.iter().filter(|v| v.valid).count();
    match view.status {
        ProposalStatus::Ready => {
            ctx.outro(&format!(
                "{verb} · {approvals} of {} needed · {}",
                view.threshold,
                theme::ok("ready to execute")
            ));
            ctx.next(&format!("execute {short}"));
        }
        ProposalStatus::Blocked => {
            ctx.outro(&format!("{verb} · {}", theme::bad("it can no longer pass")));
        }
        _ => {
            ctx.outro(&format!(
                "{verb} · {approvals} of {} needed · waiting for {} more",
                view.threshold,
                u32::try_from(approvals).map_or(0, |a| view.threshold.saturating_sub(a))
            ));
            ctx.next(&format!("proposal show {short}"));
        }
    }
    Ok(())
}

fn phase_label(phase: ExecutionPhase) -> &'static str {
    match phase {
        ExecutionPhase::Rebuilding => "Rebuilding and checking",
        ExecutionPhase::Proving => "Proving on this device",
        ExecutionPhase::Sending => "Sending",
        ExecutionPhase::Mined => "Waiting for the block",
        ExecutionPhase::Failed => "Failed",
    }
}

/// Why a proposal that is not ready cannot execute, and what to do.
fn not_executable(ctx: &Ctx, p: &ProposalView) -> Error {
    let short = format::id(&p.id);
    let approvals = p.approvals.iter().filter(|v| v.valid).count();
    let (status, _) = describe::status(p);
    match p.status {
        ProposalStatus::Collecting => Error::new(
            crate::error::Exit::NotReady,
            format!("It has {approvals} of {} approvals.", p.threshold),
        )
        .hint(format!(
            "Ask another member to run `ome approve {short}`, or see who has: `{}`.",
            ctx.ome(&format!("proposal show {short}"))
        )),
        _ => Error::usage(format!("It is {status}.")),
    }
}

pub(crate) async fn execute(ctx: &Ctx, args: ExecuteArgs) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let p = refs::proposal(ctx, vault.address, args.proposal.as_deref(), |p| {
        p.status == ProposalStatus::Ready
    })
    .await?;
    let short = format::id(&p.id);
    if p.status != ProposalStatus::Ready {
        return Err(not_executable(ctx, &p).context(format!("Can't execute {short}")));
    }
    ctx.intro(&format!("Execute {short} · {}", describe::intent(&p)));
    let client = ctx.client().await?;
    let mut events = client.events().await?;
    let call = {
        let client = client.clone();
        let target = ProposalRef {
            vault: vault.address,
            id: p.id,
        };
        tokio::spawn(async move { client.call::<ProposalsExecute>(&target).await })
    };
    tokio::pin!(call);
    let started = std::time::Instant::now();
    let mut step = Step::start(ctx, phase_label(ExecutionPhase::Rebuilding));
    let mut phase = ExecutionPhase::Rebuilding;
    let mut tx: Option<Hex32> = None;
    let result = loop {
        tokio::select! {
            result = &mut call => break result.map_err(|e| Error::failed(e.to_string()))?,
            event = events.next() => {
                let Some(Ok(DaemonEvent::Execution { execution })) = event else { continue };
                if execution.proposal != p.id || execution.phase == phase {
                    continue;
                }
                if let Some(hash) = execution.tx_hash {
                    tx = Some(hash);
                }
                let done = match phase {
                    ExecutionPhase::Rebuilding => "Rebuilt and checked".to_owned(),
                    ExecutionPhase::Proving => "Proved".to_owned(),
                    ExecutionPhase::Sending => format!("Sent  {}", tx.map(|h| h.to_string()).unwrap_or_default()),
                    other => phase_label(other).to_owned(),
                };
                if execution.phase == ExecutionPhase::Failed {
                    step.fail(execution.error.as_deref().unwrap_or("Failed"));
                    step = Step::start(ctx, "Finishing");
                    phase = execution.phase;
                    continue;
                }
                step.done(&done);
                phase = execution.phase;
                step = Step::start(ctx, phase_label(phase));
                if args.no_wait && tx.is_some() {
                    step.clear();
                    if ctx.mode == Mode::Json {
                        return ctx.json(&serde_json::json!({ "proposal": p.id, "txHash": tx }));
                    }
                    ctx.out(&tx.map(|h| h.to_string()).unwrap_or_default());
                    ctx.outro("Sent; not waiting for the block");
                    ctx.next(&format!("proposal show {short}"));
                    return Ok(());
                }
            }
        }
    };
    let view = match result {
        Ok(view) => view,
        Err(error) => {
            step.fail("Not executed");
            return Err(Error::from(error).context(format!("Can't execute {short}")));
        }
    };
    let hash = view.execution.map(|e| e.tx_hash).or(tx);
    step.finish(&format!(
        "Mined  {}  {}",
        hash.map(|h| h.to_string()).unwrap_or_default(),
        theme::dim(format!("{} in all", format::elapsed(started.elapsed())))
    ));
    if ctx.mode == Mode::Json {
        return ctx.json(&view);
    }
    if let Some(hash) = hash.filter(|_| !ctx.human()) {
        ctx.out(&hash.to_string());
    }
    let held = holding(ctx, &vault, &view).await;
    ctx.outro(&held.unwrap_or_else(|| "Executed".into()));
    Ok(())
}

/// What the vault holds of the token a transfer paid, after it.
async fn holding(ctx: &Ctx, vault: &VaultSummary, view: &ProposalView) -> Option<String> {
    let Intent::Transfer { token, .. } = &view.intent else {
        return None;
    };
    let balances = ctx
        .client()
        .await
        .ok()?
        .call::<TokensList>(&VaultParams {
            vault: vault.address,
        })
        .await
        .ok()?;
    let b = balances.iter().find(|b| b.token.address == *token)?;
    Some(format!(
        "{} now holds {} {}",
        vault.name,
        theme::bold(format::amount(b.balance?.0, b.token.decimals)),
        b.token.symbol
    ))
}
