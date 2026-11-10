//! Spending records between keepers: proposals and votes reach every member and payloads follow by hash, from whichever
//! member sent the proposal; a vote that arrives before its proposal is held; forged votes and unknown spends are caught.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_api::{Hex32, keeper::KeeperEvent};
use ome_wire::{Message, messages::proposals::ProposalPayload};
use support::{body, fast_config, frame_is, vault, wait_for};

#[tokio::test(flavor = "multi_thread")]
async fn a_proposal_its_payload_and_its_votes_reach_every_member() {
    let v = vault(fast_config()).await;
    let (proposal, payload) = v.proposal(10);
    let id = proposal.id().unwrap();
    let mut second = v.keepers[1].keeper.subscribe();
    let mut third = v.keepers[2].keeper.subscribe();

    v.keepers[0]
        .keeper
        .put_payload(v.address, &body(&Message::Payload(payload.clone())))
        .await
        .unwrap();
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal.clone())))
        .await
        .unwrap();
    // A member votes on a proposal its keeper holds.
    wait_for(&mut second, |e| {
        matches!(e, KeeperEvent::Proposal { .. }).then_some(())
    })
    .await;
    v.keepers[1]
        .keeper
        .put_approval(
            v.address,
            &body(&Message::Approval(v.approval(&proposal, 1, 1))),
        )
        .await
        .unwrap();

    // The vote and the payload travel separately; wait for both.
    let (mut voted, mut paid) = (false, false);
    wait_for(&mut third, |e| {
        match e {
            KeeperEvent::Approval { slot: 1, .. } => voted = true,
            KeeperEvent::Payload { .. } => paid = true,
            _ => {}
        }
        (voted && paid).then_some(())
    })
    .await;
    let keeper = &v.keepers[2].keeper;
    assert_eq!(
        keeper.proposals(v.address).unwrap(),
        vec![body(&Message::Proposal(proposal))]
    );
    assert_eq!(
        keeper.payload(v.address, payload.hash()).unwrap(),
        Some(body(&Message::Payload(payload)))
    );
    assert_eq!(keeper.approvals(id).unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_payload_is_fetched_from_whichever_member_sent_the_proposal() {
    let v = vault(fast_config()).await;
    let (proposal, payload) = v.proposal(11);
    let mut second = v.keepers[1].keeper.subscribe();
    let mut third = v.keepers[2].keeper.subscribe();
    let third_id = v.keepers[2].keeper.identity();
    v.harness
        .net
        .set_filter(move |from, to, _| from != &third_id && to != &third_id);
    v.keepers[0]
        .keeper
        .put_payload(v.address, &body(&Message::Payload(payload.clone())))
        .await
        .unwrap();
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal)))
        .await
        .unwrap();
    wait_for(&mut second, |e| {
        matches!(e, KeeperEvent::Payload { .. }).then_some(())
    })
    .await;

    // The proposer goes offline; member 3 comes back and syncs: member 2 sends the proposal and then the payload.
    let proposer = v.keepers[0].keeper.identity();
    v.harness
        .net
        .set_filter(move |from, to, _| from != &proposer && to != &proposer);
    v.keepers[2].keeper.sync(v.address).await.unwrap();

    wait_for(&mut third, |e| {
        matches!(e, KeeperEvent::Payload { .. }).then_some(())
    })
    .await;
    assert!(
        v.keepers[2]
            .keeper
            .payload(v.address, payload.hash())
            .unwrap()
            .is_some()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vote_that_arrives_before_its_proposal_is_held_until_it_does() {
    let v = vault(fast_config()).await;
    let (proposal, _) = v.proposal(12);
    let id = proposal.id().unwrap();
    let mut first = v.keepers[0].keeper.subscribe();
    let mut second = v.keepers[1].keeper.subscribe();
    let mut third = v.keepers[2].keeper.subscribe();
    let away = v.keepers[2].keeper.identity();
    v.harness.net.set_filter(move |_, to, frame| {
        !(to == &away && frame_is(frame, |m| matches!(m, Message::Proposal(_))))
    });
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal.clone())))
        .await
        .unwrap();
    wait_for(&mut second, |e| {
        matches!(e, KeeperEvent::Proposal { .. }).then_some(())
    })
    .await;
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
        v.keepers[2].keeper.approvals(id).unwrap(),
        Vec::<Vec<u8>>::new()
    );
    v.harness.net.clear_filter();

    // The proposal finally arrives; the held approval is verified and stored with it.
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal)))
        .await
        .unwrap();

    wait_for(&mut third, |e| {
        matches!(e, KeeperEvent::Approval { slot: 1, .. }).then_some(())
    })
    .await;
    assert_eq!(v.keepers[2].keeper.approvals(id).unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn forged_or_misdirected_votes_are_refused() {
    let v = vault(fast_config()).await;
    let (proposal, _) = v.proposal(13);
    let (unknown, _) = v.proposal(14);
    let keeper = &v.keepers[0].keeper;
    keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal.clone())))
        .await
        .unwrap();

    // Signed by member 1 but claiming member 2's slot; for a proposal nobody made; and a payload of no proposal.
    let forged = keeper
        .put_approval(
            v.address,
            &body(&Message::Approval(v.approval(&proposal, 1, 0))),
        )
        .await;
    let stray = keeper
        .put_approval(
            v.address,
            &body(&Message::Approval(v.approval(&unknown, 0, 0))),
        )
        .await;

    assert!(forged.is_err());
    assert!(stray.is_err());
    assert_eq!(
        keeper.approvals(proposal.id().unwrap()).unwrap(),
        Vec::<Vec<u8>>::new()
    );
    keeper
        .put_rejection(
            v.address,
            &body(&Message::Rejection(v.rejection(&proposal, 2))),
        )
        .await
        .unwrap();
    assert_eq!(keeper.rejections(proposal.id().unwrap()).unwrap().len(), 1);
    let unrelated = ProposalPayload {
        bytes: vec![1, 2, 3],
    };
    assert!(
        keeper
            .put_payload([9; 32], &body(&Message::Payload(unrelated)))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn receipts_replicate_and_an_unknown_spend_raises_an_alarm() {
    let v = vault(fast_config()).await;
    let (proposal, _) = v.proposal(15);
    let id = proposal.id().unwrap();
    let mut second = v.keepers[1].keeper.subscribe();
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal)))
        .await
        .unwrap();

    v.keepers[0]
        .keeper
        .put_receipt(v.address, id, [7; 32], 12)
        .await
        .unwrap();

    // The receipt names the member whose keeper signed it.
    let executor = wait_for(&mut second, |e| match e {
        KeeperEvent::Receipt { executor, .. } => Some(*executor),
        _ => None,
    })
    .await;
    assert_eq!(executor, Some(1));
    assert!(v.keepers[1].keeper.observe_tx(v.address, [7; 32]).unwrap());
    assert!(!v.keepers[1].keeper.observe_tx(v.address, [8; 32]).unwrap());
    let spend = wait_for(&mut second, |e| match e {
        KeeperEvent::UnexpectedSpend { tx_hash, .. } => Some(*tx_hash),
        _ => None,
    })
    .await;
    assert_eq!(spend, Hex32([8; 32]));
}
