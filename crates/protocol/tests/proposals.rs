//! Proposals are checked against their payload, approvals verify only for their exact proposal, vault, config and
//! slot, and the tally decides exactly at the thresholds.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::Base;
use ome_protocol::{
    Error,
    proposals::{Status, Tally, check_proposal, verify_approval},
    vault::approval_message,
};
use ome_wire::types::parse_field;
use support::fixture::{NOW, approve, chain, fixture, proposed, reject, sign_as};

#[test]
fn a_built_proposal_checks_out_and_yields_its_calls() {
    let f = fixture();
    let (proposal, payload) = proposed(&f, 10);

    let request = check_proposal(&f.vault, chain(), &proposal, &payload, NOW).unwrap();

    assert_eq!(request, support::fixture::request(10));
}

#[test]
fn a_proposal_that_does_not_match_its_payload_is_refused() {
    let f = fixture();
    let (proposal, payload) = proposed(&f, 10);
    let (_, other_payload) = proposed(&f, 99);

    assert_eq!(
        check_proposal(&f.vault, chain(), &proposal, &other_payload, NOW),
        Err(Error::PayloadMismatch)
    );

    // The proposer shows the payload for 99 but collects approvals for the hash of 10.
    let mut disguised = proposal.clone();
    disguised.payload = other_payload.reference().unwrap();
    assert_eq!(
        check_proposal(&f.vault, chain(), &disguised, &other_payload, NOW),
        Err(Error::OuterHashMismatch)
    );

    let other_chain = ome_protocol::payload::ChainInfo {
        chain_id: Base::from(1u64),
        ..chain()
    };
    assert_eq!(
        check_proposal(&f.vault, other_chain, &proposal, &payload, NOW),
        Err(Error::OuterHashMismatch)
    );
}

#[test]
fn stale_expired_foreign_and_publicly_paid_proposals_are_refused() {
    let f = fixture();
    let (proposal, payload) = proposed(&f, 10);
    let check = |change: fn(&mut ome_wire::messages::proposals::Proposal)| {
        let mut changed = proposal.clone();
        change(&mut changed);
        check_proposal(&f.vault, chain(), &changed, &payload, NOW)
    };

    assert_eq!(
        check(|p| p.config_version = 1),
        Err(Error::StaleConfig {
            proposed: 1,
            current: 0
        })
    );
    assert_eq!(check(|p| p.proposer = 4), Err(Error::UnknownSlot(3)));
    assert_eq!(check(|p| p.fee_method = 1), Err(Error::PublicFeePayment(1)));
    assert_eq!(
        check_proposal(&f.vault, chain(), &proposal, &payload, NOW + 3601),
        Err(Error::Expired)
    );
}

#[test]
fn an_approval_verifies_only_for_its_proposal_version_and_slot() {
    let mut f = fixture();
    let (proposal, _) = proposed(&f, 10);
    let (other, _) = proposed(&f, 11);
    let approval = approve(&mut f, &proposal, 1);

    assert_eq!(verify_approval(&f.vault, &proposal, &approval), Ok(()));
    assert_eq!(
        verify_approval(&f.vault, &other, &approval),
        Err(Error::WrongProposal)
    );

    let in_other_slot = ome_wire::messages::proposals::Approval {
        slot: 2,
        ..approval
    };
    assert_eq!(
        verify_approval(&f.vault, &proposal, &in_other_slot),
        Err(Error::InvalidSignature(2))
    );
    let past_the_set = ome_wire::messages::proposals::Approval {
        slot: 3,
        ..approval
    };
    assert_eq!(
        verify_approval(&f.vault, &proposal, &past_the_set),
        Err(Error::UnknownSlot(3))
    );

    let outer = parse_field(&proposal.outer_hash).unwrap();
    let next_version = approval_message(f.vault.address(), outer, 1, proposal.expires_at);
    let stale = sign_as(&mut f, 1, next_version, &proposal);
    assert_eq!(
        verify_approval(&f.vault, &proposal, &stale),
        Err(Error::InvalidSignature(1))
    );
}

#[test]
fn the_tally_approves_at_exactly_k_and_ignores_repeat_votes() {
    let mut f = fixture();
    let (proposal, _) = proposed(&f, 10);
    let first = approve(&mut f, &proposal, 2);
    let second = approve(&mut f, &proposal, 0);
    let vault = f.vault.clone();
    let mut tally = Tally::new(&vault, &proposal).unwrap();

    assert_eq!(tally.add_approval(&first), Ok(true));
    assert_eq!(tally.add_approval(&first), Ok(false));
    assert_eq!(
        tally.status(NOW),
        Status::Pending {
            approvals: 1,
            rejections: 0,
            needed: 2
        }
    );
    assert!(tally.witness().is_none());

    assert_eq!(tally.add_approval(&second), Ok(true));
    assert_eq!(tally.status(NOW), Status::Approved);
    assert!(tally.witness().is_some());
    assert_eq!(tally.status(NOW + 3601), Status::Expired);
}

#[test]
fn the_tally_blocks_once_k_is_out_of_reach() {
    let mut f = fixture();
    let (proposal, _) = proposed(&f, 10);
    let rejections = [reject(&mut f, &proposal, 0), reject(&mut f, &proposal, 1)];
    let late_approval = approve(&mut f, &proposal, 1);
    let vault = f.vault.clone();
    let mut tally = Tally::new(&vault, &proposal).unwrap();

    // 2-of-3: one rejection still leaves two possible approvals; two do not.
    assert_eq!(tally.add_rejection(&rejections[0]), Ok(true));
    assert!(matches!(
        tally.status(NOW),
        Status::Pending { rejections: 1, .. }
    ));
    assert_eq!(tally.add_rejection(&rejections[1]), Ok(true));
    assert_eq!(tally.status(NOW), Status::Blocked);
    assert_eq!(tally.add_approval(&late_approval), Ok(false));
}
