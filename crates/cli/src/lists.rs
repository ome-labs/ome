//! The daemon's paged lists, read whole: references, counts and the watch view need every item, so these follow
//! `next` until the last page. `max` stops early for views that show only the newest.

use ome_api::{
    Hex32,
    daemon::{
        ActivityEntry, ActivityParams, AuditView, Counterparty, PageParams, ProposalFilter,
        ProposalListParams, ProposalView, ProposalsList, VaultActivity, VaultAudit,
        VaultCounterparties,
    },
};
use ome_client::DaemonClient;

use crate::error::Result;

/// The largest page the daemon serves.
const PAGE: u32 = 500;

/// How many to ask for next: a full page, or what is left of `max`.
fn take(max: Option<usize>, have: usize) -> Option<u32> {
    match max {
        None => Some(PAGE),
        Some(max) if have >= max => None,
        Some(max) => u32::try_from(max - have).ok().map(|left| left.min(PAGE)),
    }
}

/// The vault's proposals passing `filter`, newest first.
pub(crate) async fn proposals(
    client: &DaemonClient,
    vault: Hex32,
    filter: ProposalFilter,
) -> Result<Vec<ProposalView>> {
    let mut items = Vec::new();
    let mut before = None;
    while let Some(limit) = take(None, items.len()) {
        let page = client
            .call::<ProposalsList>(&ProposalListParams {
                vault,
                filter: Some(filter),
                limit: Some(limit),
                before,
            })
            .await?;
        items.extend(page.items);
        let Some(next) = page.next else { break };
        before = Some(next);
    }
    Ok(items)
}

/// The vault's activity, newest first, at most `max` entries.
pub(crate) async fn activity(
    client: &DaemonClient,
    vault: Hex32,
    max: Option<usize>,
) -> Result<Vec<ActivityEntry>> {
    let mut items = Vec::new();
    let mut before = None;
    while let Some(limit) = take(max, items.len()) {
        let page = client
            .call::<VaultActivity>(&ActivityParams {
                vault,
                kinds: None,
                token: None,
                limit: Some(limit),
                before,
            })
            .await?;
        items.extend(page.items);
        let Some(next) = page.next else { break };
        before = Some(next);
    }
    Ok(items)
}

/// The vault's ECDH audit log, newest first, at most `max` entries.
pub(crate) async fn audit(
    client: &DaemonClient,
    vault: Hex32,
    max: usize,
) -> Result<Vec<AuditView>> {
    let mut items = Vec::new();
    let mut before = None;
    while let Some(limit) = take(Some(max), items.len()) {
        let page = client
            .call::<VaultAudit>(&PageParams {
                vault,
                limit: Some(limit),
                before,
            })
            .await?;
        items.extend(page.items);
        let Some(next) = page.next else { break };
        before = Some(next);
    }
    Ok(items)
}

/// Everyone who paid the vault, most recent payer first.
pub(crate) async fn counterparties(
    client: &DaemonClient,
    vault: Hex32,
) -> Result<Vec<Counterparty>> {
    let mut items = Vec::new();
    let mut before = None;
    loop {
        let page = client
            .call::<VaultCounterparties>(&PageParams {
                vault,
                limit: Some(PAGE),
                before,
            })
            .await?;
        items.extend(page.items);
        let Some(next) = page.next else { break };
        before = Some(next);
    }
    Ok(items)
}
