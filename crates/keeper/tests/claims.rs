//! Execution claims: a member's keeper announces that it is executing a proposal, the other members see who and how
//! far, and a claim ends with the execution, the proposal's receipt, or silence.
#![allow(clippy::unwrap_used)]

mod support;

use ome_api::{Hex32, daemon::ExecutionPhase, keeper::KeeperEvent};
use ome_keeper::CLAIM_TTL_SECS;
use ome_wire::Message;
use support::{body, fast_config, vault, wait_for};

#[tokio::test(flavor = "multi_thread")]
async fn members_see_who_is_executing_until_it_ends_or_lapses() {
    let v = vault(fast_config()).await;
    let (proposal, _) = v.proposal(20);
    let id = proposal.id().unwrap();
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal)))
        .await
        .unwrap();
    let bob = &v.keepers[1].keeper;
    let mut events = bob.subscribe();

    let claim = |phase| v.keepers[0].keeper.put_claim(v.address, id, phase, 100);
    claim(ExecutionPhase::Proving).await.unwrap();
    let seen = wait_for(&mut events, |e| match e {
        KeeperEvent::Claim { member, phase, .. } => Some((*member, *phase)),
        _ => None,
    })
    .await;
    assert_eq!(seen, (1, ExecutionPhase::Proving));
    let claims = bob.claims(v.address).await;
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].proposal, Hex32(id));
    assert!(!claims[0].mine);
    assert!(v.keepers[0].keeper.claims(v.address).await[0].mine);

    // A failed execution ends the claim.
    claim(ExecutionPhase::Failed).await.unwrap();
    wait_for(&mut events, |e| {
        matches!(
            e,
            KeeperEvent::Claim {
                phase: ExecutionPhase::Failed,
                ..
            }
        )
        .then_some(())
    })
    .await;
    assert_eq!(bob.claims(v.address).await, Vec::new());

    // A claim nobody refreshes lapses.
    claim(ExecutionPhase::Sending).await.unwrap();
    wait_for(&mut events, |e| {
        matches!(
            e,
            KeeperEvent::Claim {
                phase: ExecutionPhase::Sending,
                ..
            }
        )
        .then_some(())
    })
    .await;
    assert_eq!(bob.claims(v.address).await.len(), 1);
    v.harness.clock.advance(CLAIM_TTL_SECS + 1);
    assert_eq!(bob.claims(v.address).await, Vec::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_receipt_ends_the_claims_on_its_proposal() {
    let v = vault(fast_config()).await;
    let (proposal, _) = v.proposal(21);
    let id = proposal.id().unwrap();
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal)))
        .await
        .unwrap();
    let alice = &v.keepers[0].keeper;
    alice
        .put_claim(v.address, id, ExecutionPhase::Proving, 100)
        .await
        .unwrap();
    assert_eq!(alice.claims(v.address).await.len(), 1);
    alice.put_receipt(v.address, id, [7; 32], 12).await.unwrap();
    assert_eq!(alice.claims(v.address).await, Vec::new());
}
