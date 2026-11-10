//! Shared names: a member renames the vault or themselves, every member's keeper sees the newest name with who set
//! it, and a member that was away gets the current names through sync.
#![allow(clippy::unwrap_used)]

mod support;

use ome_api::keeper::{KeeperEvent, NameSubject};
use support::{fast_config, vault, wait_for};

#[tokio::test(flavor = "multi_thread")]
async fn renames_reach_every_member_and_the_newest_wins() {
    let v = vault(fast_config()).await;
    let away = v.keepers[2].keeper.identity();
    v.harness
        .net
        .set_filter(move |from, to, _| from != &away && to != &away);
    let mut bob = v.keepers[1].keeper.subscribe();

    v.keepers[0]
        .keeper
        .put_name(v.address, NameSubject::Vault, "Operations".into())
        .await
        .unwrap();
    let seen = wait_for(&mut bob, |e| match e {
        KeeperEvent::Name { name, .. } => Some(name.clone()),
        _ => None,
    })
    .await;
    assert_eq!((seen.name.as_str(), seen.member), ("Operations", Some(1)));

    v.harness.clock.advance(5);
    v.keepers[1]
        .keeper
        .put_name(v.address, NameSubject::Member, "Robert".into())
        .await
        .unwrap();
    v.keepers[1]
        .keeper
        .put_name(v.address, NameSubject::Vault, "Ops".into())
        .await
        .unwrap();
    let names = v.keepers[1].keeper.names(v.address).await.unwrap();
    let vault_name = names
        .iter()
        .find(|n| n.subject == NameSubject::Vault)
        .unwrap();
    assert_eq!(
        (vault_name.name.as_str(), vault_name.member),
        ("Ops", Some(2))
    );
    assert!(
        names
            .iter()
            .any(|n| n.subject == NameSubject::Member && n.name == "Robert" && n.member == Some(2))
    );

    // Carol was away: a sync brings her the current names.
    let mut carol = v.keepers[2].keeper.subscribe();
    v.harness.net.clear_filter();
    v.keepers[2].keeper.sync(v.address).await.unwrap();
    let (mut vault_seen, mut member_seen) = (false, false);
    wait_for(&mut carol, |e| {
        if let KeeperEvent::Name { name, .. } = e {
            vault_seen |= name.name == "Ops";
            member_seen |= name.name == "Robert";
        }
        (vault_seen && member_seen).then_some(())
    })
    .await;
    assert_eq!(v.keepers[2].keeper.names(v.address).await.unwrap().len(), 2);
}
