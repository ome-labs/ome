//! `ome watch`: the one full-screen view. A vault's proposals and activity, live from the daemon's events, with
//! single keys to act. Acting leaves the full screen for the usual clear-signing flow, then comes back, so nothing is
//! ever signed from a screen that hides the review.

use std::{
    io::BufRead,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use ome_api::{
    Empty,
    daemon::{
        ActivityEntry, DaemonEvent, ProposalStatus, ProposalView, ProposalsList, ServiceStatus,
        Status, StatusGet, VaultActivity, VaultList, VaultSummary, VaultUse,
    },
    keeper::VaultParams,
};
use ratatui::{
    Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
};
use tokio::sync::mpsc;

use crate::{
    args::{ExecuteArgs, ProposalArg, ProposeCommand, ProposeOptions, RejectArgs},
    commands::{proposal, propose},
    context::Ctx,
    error::{Error, Result},
    refs::{self, Book},
    ui::{describe, format},
};

struct View {
    vault: VaultSummary,
    status: Status,
    proposals: Vec<ProposalView>,
    activity: Vec<ActivityEntry>,
    book: Book,
    list: ListState,
    message: String,
}

impl View {
    fn selected(&self) -> Option<&ProposalView> {
        self.list.selected().and_then(|i| self.proposals.get(i))
    }
}

/// Reads keys on a thread, paused while the terminal is handed to a prompt.
fn keys(paused: Arc<AtomicBool>) -> mpsc::UnboundedReceiver<Event> {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        loop {
            if paused.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            if event::poll(Duration::from_millis(100)).unwrap_or(false)
                && let Ok(event) = event::read()
                && tx.send(event).is_err()
            {
                return;
            }
        }
    });
    rx
}

#[expect(clippy::too_many_lines, reason = "a guided flow reads top to bottom")]
pub(crate) async fn run(ctx: &mut Ctx) -> Result<()> {
    if !ctx.interactive {
        return Err(Error::usage("`ome watch` needs a terminal.")
            .hint("For scripts, follow the activity instead: `ome activity --follow --plain`."));
    }
    let mut vault = refs::vault(ctx).await?;
    // Commands run from the view act on the vault shown.
    ctx.vault = Some(vault.address.to_string());
    let client = ctx.client().await?.clone();
    let mut events = client.events().await?;
    let mut view = load(ctx, vault.clone()).await?;
    let paused = Arc::new(AtomicBool::new(false));
    let mut input = keys(paused.clone());
    let mut terminal = ratatui::init();
    let result = loop {
        if let Err(e) = terminal.draw(|frame| draw(frame, &mut view)) {
            break Err(Error::failed(e.to_string()));
        }
        let refresh = tokio::select! {
            key = input.recv() => {
                let Some(Event::Key(key)) = key else { continue };
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => {
                        view.list.select_next();
                        continue;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        view.list.select_previous();
                        continue;
                    }
                    KeyCode::Char('v') => {
                        let vaults = client.call::<VaultList>(&Empty {}).await?;
                        if let Some(i) = vaults.iter().position(|v| v.address == vault.address) {
                            vault = vaults[(i + 1) % vaults.len()].clone();
                            client.call::<VaultUse>(&VaultParams { vault: vault.address }).await?;
                            ctx.vault = Some(vault.address.to_string());
                        }
                        true
                    }
                    KeyCode::Char('R') => true,
                    code => {
                        let Some(action) = Action::from(code, view.selected()) else { continue };
                        paused.store(true, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        ratatui::restore();
                        let outcome = action.run(ctx).await;
                        view.message = match &outcome {
                            Ok(()) => format!("{} done", action.name()),
                            Err(e) => format!("{}: {}", action.name(), e.message),
                        };
                        if let Err(e) = &outcome {
                            eprintln!("✗ {}", e.message);
                            if let Some(hint) = &e.hint {
                                eprintln!("  {hint}");
                            }
                        }
                        eprintln!("\n  Press Enter to go back to the view.");
                        let _ = std::io::stdin().lock().read_line(&mut String::new());
                        terminal = ratatui::init();
                        paused.store(false, Ordering::SeqCst);
                        true
                    }
                }
            }
            event = events.next() => match event {
                Some(Ok(DaemonEvent::VaultActivity { vault: v })) => v == vault.address,
                Some(Ok(DaemonEvent::Signer { signer })) => { view.status.signer = signer; false }
                Some(Ok(DaemonEvent::Node { node })) => { view.status.node = node; false }
                Some(Ok(DaemonEvent::Keeper { keeper })) => { view.status.keeper = keeper; false }
                Some(Ok(DaemonEvent::Execution { execution })) if execution.vault == vault.address => {
                    view.message = format!("{}: {:?}", format::id(&execution.proposal), execution.phase).to_lowercase();
                    false
                }
                Some(Ok(DaemonEvent::HandshakeAdvised { vault: v, counterparties })) if v == vault.address => {
                    view.message = format!("{counterparties} counterparties should handshake again (ome counterparties)");
                    false
                }
                Some(_) => false,
                None => break Err(Error::new(crate::error::Exit::Daemon, "The daemon closed the connection.")),
            },
            () = tokio::time::sleep(Duration::from_secs(10)) => true,
        };
        if refresh {
            let message = std::mem::take(&mut view.message);
            let selected = view.list.selected();
            match load(ctx, vault.clone()).await {
                Ok(fresh) => {
                    view = fresh;
                    view.message = message;
                    view.list.select(selected.or(Some(0)));
                }
                Err(e) => view.message = e.message,
            }
        }
    };
    ratatui::restore();
    result
}

async fn load(ctx: &Ctx, vault: VaultSummary) -> Result<View> {
    let client = ctx.client().await?;
    let params = VaultParams {
        vault: vault.address,
    };
    let (status, proposals, activity) = tokio::join!(
        client.call::<StatusGet>(&Empty {}),
        client.call::<ProposalsList>(&params),
        client.call::<VaultActivity>(&params),
    );
    let mut proposals = proposals?;
    // Open proposals first, then the rest, newest first within each.
    proposals.sort_by_key(|p| {
        (
            !describe::is_open(p.status),
            std::cmp::Reverse(p.expires_at),
        )
    });
    let mut list = ListState::default();
    if !proposals.is_empty() {
        list.select(Some(0));
    }
    Ok(View {
        vault,
        status: status?,
        proposals,
        activity: activity?,
        book: Book::load(ctx).await?,
        list,
        message: String::new(),
    })
}

/// What a key does to the selected proposal, run outside the full screen.
enum Action {
    Review(String),
    Approve(String),
    Reject(String),
    Execute(String),
    Propose,
}

impl Action {
    fn from(code: KeyCode, selected: Option<&ProposalView>) -> Option<Self> {
        if code == KeyCode::Char('p') {
            return Some(Self::Propose);
        }
        let id = selected?.id.to_string();
        Some(match code {
            KeyCode::Enter => Self::Review(id),
            KeyCode::Char('a') => Self::Approve(id),
            KeyCode::Char('r') => Self::Reject(id),
            KeyCode::Char('x') => Self::Execute(id),
            _ => return None,
        })
    }

    const fn name(&self) -> &'static str {
        match self {
            Self::Review(_) => "review",
            Self::Approve(_) => "approve",
            Self::Reject(_) => "reject",
            Self::Execute(_) => "execute",
            Self::Propose => "propose",
        }
    }

    async fn run(&self, ctx: &Ctx) -> Result<()> {
        let arg = |id: &String| ProposalArg {
            proposal: Some(id.clone()),
        };
        match self {
            Self::Review(id) => proposal::review(ctx, arg(id)).await,
            Self::Approve(id) => proposal::approve(ctx, arg(id)).await,
            Self::Reject(id) => {
                proposal::reject(
                    ctx,
                    RejectArgs {
                        proposal: Some(id.clone()),
                        reason: None,
                    },
                )
                .await
            }
            Self::Execute(id) => {
                proposal::execute(
                    ctx,
                    ExecuteArgs {
                        proposal: Some(id.clone()),
                        no_wait: false,
                    },
                )
                .await
            }
            Self::Propose => {
                let transfer = ProposeCommand::Transfer {
                    amount: None,
                    token: None,
                    to: None,
                    options: ProposeOptions {
                        memo: None,
                        expires: "24h".into(),
                        dry_run: false,
                    },
                };
                propose::run(ctx, transfer).await
            }
        }
    }
}

#[expect(clippy::too_many_lines, reason = "a guided flow reads top to bottom")]
fn draw(frame: &mut Frame<'_>, view: &mut View) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    frame.render_widget(Paragraph::new(header_line(view)), header);

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(body);
    let items: Vec<ListItem<'_>> = view
        .proposals
        .iter()
        .map(|p| {
            let (status, tone) = describe::status(p);
            let colour = match tone {
                crate::ui::table::Tone::Ok => Color::Green,
                crate::ui::table::Tone::Warn => Color::Yellow,
                crate::ui::table::Tone::Bad => Color::Red,
                _ => Color::DarkGray,
            };
            let when = if describe::is_open(p.status) {
                format::relative(p.expires_at)
            } else {
                String::new()
            };
            let mut spans = vec![
                Span::styled(
                    format!("{}  ", &format::id(&p.id)[..4]),
                    Style::new().fg(Color::Cyan),
                ),
                Span::raw(describe::intent_with(p, &|a| view.book.label(a))),
                Span::raw("  "),
                Span::styled(status, Style::new().fg(colour)),
                Span::styled(
                    format!("  {when}"),
                    Style::new().add_modifier(Modifier::DIM),
                ),
            ];
            if describe::can_vote(p) {
                spans.push(Span::styled(
                    "  ● your vote",
                    Style::new().fg(Color::Yellow),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let empty = items.is_empty();
    let list = List::new(items)
        .block(Block::bordered().title(" Proposals "))
        .highlight_symbol("▸ ")
        .highlight_style(Style::new().add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(list, left, &mut view.list);
    if empty {
        let inner = left.inner(ratatui::layout::Margin::new(2, 1));
        frame.render_widget(
            Paragraph::new("No proposals yet. Press p to propose a payment.")
                .style(Style::new().add_modifier(Modifier::DIM)),
            inner,
        );
    }

    let activity: Vec<ListItem<'_>> = view
        .activity
        .iter()
        .take(usize::from(right.height))
        .map(|entry| {
            let (at, text) = match entry {
                ActivityEntry::Proposal { at, proposal } => (
                    *at,
                    format!(
                        "{} {} · {}",
                        &format::id(&proposal.id)[..4],
                        describe::intent(proposal),
                        describe::status(proposal).0
                    ),
                ),
                ActivityEntry::Received {
                    at,
                    token,
                    from,
                    amount,
                    ..
                } => (
                    *at,
                    format!(
                        "received {} {} from {}",
                        format::amount(amount.0, token.decimals),
                        token.symbol,
                        view.book.label(from).unwrap_or_else(|| format::short(from))
                    ),
                ),
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>9}  ", format::relative(at)),
                    Style::new().add_modifier(Modifier::DIM),
                ),
                Span::raw(text),
            ]))
        })
        .collect();
    frame.render_widget(
        List::new(activity).block(Block::bordered().title(" Activity ")),
        right,
    );

    let keys = " enter review   a approve   r reject   x execute   p propose   v vault   q quit";
    let line = if view.message.is_empty() {
        Line::from(Span::styled(keys, Style::new().add_modifier(Modifier::DIM)))
    } else {
        Line::from(vec![
            Span::styled(format!(" {}", view.message), Style::new().fg(Color::Yellow)),
            Span::styled(
                format!("   ·{keys}"),
                Style::new().add_modifier(Modifier::DIM),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(line), footer);
}

fn header_line(view: &View) -> Line<'static> {
    let v = &view.vault;
    let s = &view.status;
    let ok = |good: bool| {
        if good {
            Span::styled("✓", Style::new().fg(Color::Green))
        } else {
            Span::styled("✗", Style::new().fg(Color::Red))
        }
    };
    let signer = match s.signer.unlocked_until {
        Some(until) if !s.signer.locked => format!(
            "unlocked {}",
            format::span(until.saturating_sub(format::now()))
        ),
        _ => "locked".into(),
    };
    Line::from(vec![
        Span::styled(
            format!(" Ome · {}", v.name),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "    {}    ",
            describe::shape(v.threshold, v.viewing_threshold, v.members)
        )),
        Span::raw("node "),
        ok(s.node.reachable),
        Span::raw(format!(
            " {}    keeper ",
            describe::network(s.node.l1_chain_id, s.node.rollup_version)
        )),
        ok(s.keeper.status == ServiceStatus::Running),
        Span::raw(format!("    signer {signer}")),
        Span::styled(
            match view
                .proposals
                .iter()
                .filter(|p| p.status == ProposalStatus::Ready)
                .count()
            {
                0 => String::new(),
                n => format!("    {n} ready"),
            },
            Style::new().fg(Color::Green),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use ome_api::{
        Hex32,
        daemon::{KeeperState, NodeStatus, ServiceStatus, Settings, SignerStatus, Status},
    };
    use ratatui::{Terminal, backend::TestBackend, widgets::ListState};

    use super::{View, draw};
    use crate::{fixtures, refs::Book};

    fn status() -> Status {
        Status {
            profile: None,
            settings: Settings {
                node_url: "http://localhost:8080".into(),
                relays: vec![],
                prover_enabled: false,
                unlock_timeout_secs: 3600,
                fee_payer_salt: Hex32([0; 32]),
                remote_keeper: None,
                aztec_wallet_dir: None,
                aztec_wallet_sync: false,
            },
            keeper: KeeperState {
                status: ServiceStatus::Running,
                identity: None,
                peer_id: None,
                addresses: vec![],
                version: None,
                error: None,
                remote: None,
            },
            node: NodeStatus {
                url: "http://localhost:8080".into(),
                reachable: true,
                node_version: None,
                l1_chain_id: Some(31_337),
                rollup_version: Some(1),
                block_number: Some(7),
                synced_block: Some(7),
                checked_at: 0,
                error: None,
            },
            signer: SignerStatus {
                locked: true,
                unlocked_until: None,
            },
            vaults: vec![],
            default_vault: None,
        }
    }

    #[test]
    fn the_view_shows_proposals_votes_and_keys() {
        let vault = ome_api::daemon::VaultSummary {
            address: fixtures::hex("7a11", "01"),
            name: "Treasury".into(),
            alias: None,
            is_default: true,
            threshold: 2,
            viewing_threshold: 2,
            members: 3,
            my_index: 2,
            founded_at: 0,
        };
        let mut view = View {
            vault,
            status: status(),
            proposals: vec![fixtures::proposal()],
            activity: vec![],
            book: Book::default(),
            list: ListState::default().with_selected(Some(0)),
            message: String::new(),
        };
        let mut terminal = Terminal::new(TestBackend::new(118, 8)).unwrap();
        terminal.draw(|frame| draw(frame, &mut view)).unwrap();
        let text = format!("{}", terminal.backend());
        assert!(text.contains("Ome · Treasury"), "{text}");
        assert!(text.contains("pay 250 USDC → 0x2b1f…9e04"), "{text}");
        assert!(text.contains("your vote"), "{text}");
        assert!(text.contains("a approve"), "{text}");
    }
}
