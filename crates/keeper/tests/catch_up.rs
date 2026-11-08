//! Catching up: an offline member gets what it missed through sync or through its relay mailbox, and an equivocating
//! dealer is caught by the echo.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_api::keeper::{CeremonyStatus, KeeperEvent};
use ome_keeper::{
    Clock,
    ceremony::dkg::{Dkg, DkgParams},
};
use ome_net::Transport;
use ome_wire::{Header, Message, SignedMessage, encode, version::PROTOCOL_VERSION};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use support::{Harness, TestKeeper, WAIT, body, fast_config, vault, vault_on, wait_for};

/// Cuts member 3 off, then member 1 proposes with a payload and member 2 approves.
async fn records_while_member_3_is_away(v: &support::Vault) -> [u8; 32] {
    let away = v.keepers[2].keeper.identity();
    v.harness
        .net
        .set_filter(move |from, to, _| from != &away && to != &away);
    let (proposal, payload) = v.proposal(25);
    let mut first = v.keepers[0].keeper.subscribe();
    v.keepers[0]
        .keeper
        .put_payload(v.address, &body(&Message::Payload(payload)))
        .await
        .unwrap();
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal.clone())))
        .await
        .unwrap();
    v.keepers[1]
        .keeper
        .put_approval(
            v.address,
            &body(&Message::Approval(v.approval(&proposal, 1, 1))),
        )
        .await
        .unwrap();
    wait_for(&mut first, |e| {
        matches!(e, KeeperEvent::Approval { .. }).then_some(())
    })
    .await;
    assert_eq!(
        v.keepers[2].keeper.proposals(v.address).unwrap(),
        Vec::<Vec<u8>>::new()
    );
    proposal.id().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_offline_member_catches_up_through_sync() {
    let v = vault(fast_config()).await;
    let id = records_while_member_3_is_away(&v).await;
    let mut third = v.keepers[2].keeper.subscribe();
    v.harness.net.clear_filter();

    v.keepers[2].keeper.sync(v.address).await.unwrap();

    // The approval and the payload arrive in either order.
    let (mut approval, mut payload) = (false, false);
    wait_for(&mut third, |e| {
        approval |= matches!(e, KeeperEvent::Approval { .. });
        payload |= matches!(e, KeeperEvent::Payload { .. });
        (approval && payload).then_some(())
    })
    .await;
    assert_eq!(v.keepers[2].keeper.proposals(v.address).unwrap().len(), 1);
    assert_eq!(v.keepers[2].keeper.approvals(id).unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_offline_member_catches_up_through_its_relay_mailbox() {
    let v = vault_on(Harness::with_mailbox(fast_config())).await;
    let mailbox = v.harness.mailbox.clone().unwrap();
    let id = records_while_member_3_is_away(&v).await;
    tokio::time::timeout(WAIT, async {
        while mailbox.len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("deposits not made in time");
    v.harness.net.clear_filter();

    let delivered = v.keepers[2].keeper.poll_mailbox().await.unwrap();

    assert!(delivered >= 2, "{delivered}");
    assert_eq!(v.keepers[2].keeper.proposals(v.address).unwrap().len(), 1);
    assert_eq!(v.keepers[2].keeper.approvals(id).unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_equivocating_dealer_is_caught_by_the_echo() {
    let harness = Harness::new(fast_config());
    let keepers: Vec<TestKeeper> = (1..=3).map(|i| harness.keeper(i)).collect();
    let params = DkgParams {
        session: [5; 32],
        threshold: 2,
        members: keepers.iter().map(TestKeeper::participant).collect(),
    };
    let mut events: Vec<_> = keepers[..2].iter().map(|k| k.keeper.subscribe()).collect();
    for k in &keepers[..2] {
        k.keeper.start_dkg(params.clone()).await.unwrap();
    }
    // Member 3 is replaced by a dealer that shows members 1 and 2 different round-1 packages.
    let keys = keepers[2].keys();
    let (dealer, _inbox) = harness.net.endpoint(keys.identity.public());
    dealer.set_admission(keepers[..2].iter().map(TestKeeper::peer).collect(), false);
    let mut rng = ChaCha20Rng::seed_from_u64(1);
    for (n, target) in keepers[..2].iter().enumerate() {
        let (_, outbox) =
            Dkg::start(params.clone(), &keys.identity.public(), 0, 30, &mut rng).unwrap();
        let id = [u8::try_from(n).unwrap() + 1; 16];
        let header = Header {
            scope: [5; 32],
            epoch: 0,
            id,
            created_at: harness.clock.now(),
        };
        let signed = SignedMessage::sign(
            &keys.identity,
            PROTOCOL_VERSION,
            header,
            outbox.0[0].1.clone(),
        )
        .unwrap();
        dealer
            .send(target.peer(), encode(&signed).unwrap())
            .await
            .unwrap();
    }

    // Every honest member aborts; the one that compares the two signed packages blames the dealer.
    for events in &mut events {
        support::ceremony_reaches(events, [5; 32], CeremonyStatus::Aborted).await;
    }
    let blamed = hex::encode(keys.identity.public().0);
    let attributed = keepers[..2]
        .iter()
        .filter(|k| {
            k.keeper
                .ceremony([5; 32])
                .unwrap()
                .failure
                .is_some_and(|f| f.blamed == vec![blamed.clone()])
        })
        .count();
    assert!(attributed >= 1, "no member attributed the equivocation");
}
