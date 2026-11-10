//! Proposals are checked against their payload, approvals verify only for their exact proposal, vault, config and
//! slot, and the tally decides exactly at the thresholds.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::Base;
use ome_protocol::payload::{CallKind, ExecutionRequest};
use ome_protocol::{
    Error,
    proposals::{Status, Tally, approval_messages_for, check_proposal, verify_approval},
    vault::approval_message,
};
use ome_wire::types::{field, parse_field};
use support::fixture::{
    NOW, approve, chain, fixture, proposed, reject, sign_as, swap_draft, swap_request,
};

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
        Err(Error::Expired {
            at: proposal.expires_at
        })
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
        ..approval.clone()
    };
    assert_eq!(
        verify_approval(&f.vault, &proposal, &in_other_slot),
        Err(Error::InvalidSignature(2))
    );
    let past_the_set = ome_wire::messages::proposals::Approval {
        slot: 3,
        ..approval.clone()
    };
    assert_eq!(
        verify_approval(&f.vault, &proposal, &past_the_set),
        Err(Error::UnknownSlot(3))
    );

    let outer = parse_field(&proposal.outer_hash).unwrap();
    let next_version = approval_message(f.vault.address(), outer, 1, proposal.expires_at);
    let stale = sign_as(&mut f, 1, &[next_version], &proposal);
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

#[test]
fn a_proposal_carries_its_authwit_hashes_and_checks_out() {
    let f = fixture();
    let (proposal, payload) = swap_draft(swap_request(10))
        .build(&f.vault, chain())
        .unwrap();

    assert_eq!(proposal.authwits.len(), 1);
    let request = check_proposal(&f.vault, chain(), &proposal, &payload, NOW).unwrap();
    assert_eq!(
        request.authwit_hashes(chain()).unwrap(),
        vec![parse_field(&proposal.authwits[0]).unwrap()]
    );

    // A proposer cannot show one authwit and collect approvals for another.
    let mut swapped = proposal.clone();
    swapped.authwits = vec![proposal.outer_hash];
    assert_eq!(
        check_proposal(&f.vault, chain(), &swapped, &payload, NOW),
        Err(Error::AuthwitMismatch)
    );
    let mut dropped = proposal;
    dropped.authwits.clear();
    assert_eq!(
        check_proposal(&f.vault, chain(), &dropped, &payload, NOW),
        Err(Error::AuthwitMismatch)
    );
}

#[test]
fn an_authwit_is_granted_only_to_a_called_contract_and_never_to_the_vault() {
    let f = fixture();
    let vault = field(&f.vault.address());
    let build = |change: &dyn Fn(&mut ExecutionRequest)| {
        let mut request = swap_request(10);
        change(&mut request);
        swap_draft(request).build(&f.vault, chain())
    };

    assert_eq!(
        build(&|r| r.authwits[0].caller = field(&Base::from(0x6666u64))).map(|_| ()),
        Err(Error::InvalidAuthwit {
            index: 0,
            reason: "the caller is not called by the transaction"
        })
    );
    assert_eq!(
        build(&|r| r.authwits[0].call.to = vault).map(|_| ()),
        Err(Error::InvalidAuthwit {
            index: 0,
            reason: "the vault cannot consume its own authwit"
        })
    );
    let to_self = |r: &mut ExecutionRequest| {
        r.calls[0].to = vault;
        r.authwits[0].caller = vault;
    };
    assert_eq!(
        build(&to_self).map(|_| ()),
        Err(Error::InvalidAuthwit {
            index: 0,
            reason: "the vault needs no authwit for its own calls"
        })
    );
    assert_eq!(
        build(&|r| r.authwits[0].call.kind = CallKind::Public).map(|_| ()),
        Err(Error::InvalidAuthwit {
            index: 0,
            reason: "a private authwit authorizes a private call"
        })
    );
    assert_eq!(
        build(&|r| r.authwits = vec![r.authwits[0].clone(); 5]).map(|_| ()),
        Err(Error::TooManyAuthwits(5))
    );
}

#[test]
fn an_approval_signs_the_transaction_and_every_authwit() {
    let mut f = fixture();
    let (proposal, _) = swap_draft(swap_request(10))
        .build(&f.vault, chain())
        .unwrap();
    let messages = approval_messages_for(&f.vault, &proposal).unwrap();
    assert_eq!(messages.len(), 2);

    let approval = approve(&mut f, &proposal, 0);
    assert_eq!(verify_approval(&f.vault, &proposal, &approval), Ok(()));

    let transaction_only = sign_as(&mut f, 0, &messages[..1], &proposal);
    assert_eq!(
        verify_approval(&f.vault, &proposal, &transaction_only),
        Err(Error::SignatureCount {
            slot: 0,
            expected: 2,
            got: 1
        })
    );
    let reordered = sign_as(&mut f, 0, &[messages[1], messages[0]], &proposal);
    assert_eq!(
        verify_approval(&f.vault, &proposal, &reordered),
        Err(Error::InvalidSignature(0))
    );
}

#[test]
fn the_tally_builds_a_witness_for_each_authwit() {
    let mut f = fixture();
    let (proposal, _) = swap_draft(swap_request(10))
        .build(&f.vault, chain())
        .unwrap();
    let first = approve(&mut f, &proposal, 0);
    let second = approve(&mut f, &proposal, 2);
    let vault = f.vault.clone();
    let mut tally = Tally::new(&vault, &proposal).unwrap();

    tally.add_approval(&first).unwrap();
    assert!(tally.authwit_witnesses().is_none());
    tally.add_approval(&second).unwrap();

    let witnesses = tally.authwit_witnesses().unwrap();
    assert_eq!(witnesses.len(), 1);
    assert_eq!(witnesses[0].0, parse_field(&proposal.authwits[0]).unwrap());
    assert_ne!(Some(&witnesses[0].1), tally.witness().as_ref());
}
