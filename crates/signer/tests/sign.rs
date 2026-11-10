//! Approvals and rejections come only from proposals `ome-protocol` accepts, and verify for exactly their proposal.

mod support;

use ome_crypto::Base;
use ome_protocol::{
    Error as ProtocolError,
    payload::{Authwit, Call},
    proposals::{Status, Tally, verify_approval, verify_rejection},
};
use ome_signer::{Error, SigningKey, approve, reject};
use ome_wire::{limits::MAX_REASON_BYTES, types::field};
use support::{NOW, chain, fixture, propose, propose_with, transfer};

#[test]
fn two_members_approve_and_the_vault_is_authorized() {
    let mut f = fixture();
    let (proposal, payload) = propose(&f, vec![transfer(10)]);
    let first = approve(
        &f.members[0],
        &f.vault,
        chain(),
        &proposal,
        &payload,
        NOW,
        &mut f.rng,
    )
    .unwrap();
    let second = approve(
        &f.members[2],
        &f.vault,
        chain(),
        &proposal,
        &payload,
        NOW,
        &mut f.rng,
    )
    .unwrap();

    let mut tally = Tally::new(&f.vault, &proposal).unwrap();
    tally.add_approval(&first).unwrap();
    tally.add_approval(&second).unwrap();

    assert_eq!((first.slot, second.slot), (0, 2));
    assert_eq!(tally.status(NOW), Status::Approved);
}

#[test]
fn an_approval_does_not_carry_over_to_another_proposal() {
    let mut f = fixture();
    let (proposal, payload) = propose(&f, vec![transfer(10)]);
    let (other, _) = propose(&f, vec![transfer(11)]);

    let approval = approve(
        &f.members[1],
        &f.vault,
        chain(),
        &proposal,
        &payload,
        NOW,
        &mut f.rng,
    )
    .unwrap();

    assert_eq!(verify_approval(&f.vault, &proposal, &approval), Ok(()));
    assert_eq!(
        verify_approval(&f.vault, &other, &approval),
        Err(ProtocolError::WrongProposal)
    );
}

#[test]
fn an_approval_signs_every_authwit_the_proposal_grants() {
    let mut f = fixture();
    let amm = transfer(1);
    let authwit = Authwit {
        caller: amm.to,
        call: Call {
            to: field(&Base::from(0x7001u64)),
            ..transfer(10)
        },
    };
    let (proposal, payload) = propose_with(&f, vec![amm], vec![authwit]);

    let approval = approve(
        &f.members[1],
        &f.vault,
        chain(),
        &proposal,
        &payload,
        NOW,
        &mut f.rng,
    )
    .unwrap();

    assert_eq!(approval.signatures.len(), 2);
    assert_eq!(verify_approval(&f.vault, &proposal, &approval), Ok(()));
}

#[test]
fn a_proposal_protocol_refuses_is_never_signed() {
    let mut f = fixture();
    let (proposal, payload) = propose(&f, vec![transfer(10)]);
    let (_, other_payload) = propose(&f, vec![transfer(99)]);
    let mut public_fee = proposal.clone();
    public_fee.fee_method = 1;
    let mut sign = |proposal, payload, now| {
        approve(
            &f.members[0],
            &f.vault,
            chain(),
            proposal,
            payload,
            now,
            &mut f.rng,
        )
    };

    assert_eq!(
        sign(&proposal, &other_payload, NOW).err(),
        Some(Error::Protocol(ProtocolError::PayloadMismatch))
    );
    assert_eq!(
        sign(&proposal, &payload, NOW + 3601).err(),
        Some(Error::Protocol(ProtocolError::Expired {
            at: proposal.expires_at
        }))
    );
    assert_eq!(
        sign(&public_fee, &payload, NOW).err(),
        Some(Error::Protocol(ProtocolError::PublicFeePayment(1)))
    );
}

#[test]
fn a_key_outside_the_signer_set_cannot_vote() {
    let mut f = fixture();
    let (proposal, payload) = propose(&f, vec![transfer(10)]);
    let stranger = SigningKey::generate(&mut f.rng).unwrap();

    assert_eq!(
        approve(
            &stranger,
            &f.vault,
            chain(),
            &proposal,
            &payload,
            NOW,
            &mut f.rng
        )
        .err(),
        Some(Error::NotASigner)
    );
    assert_eq!(
        reject(&stranger, &f.vault, &proposal, "", &mut f.rng).err(),
        Some(Error::NotASigner)
    );
}

#[test]
fn a_rejection_verifies_and_its_reason_is_bounded() {
    let mut f = fixture();
    let (proposal, _) = propose(&f, vec![transfer(10)]);

    let rejection = reject(
        &f.members[1],
        &f.vault,
        &proposal,
        "wrong recipient",
        &mut f.rng,
    )
    .unwrap();

    assert_eq!(
        verify_rejection(&f.vault, &proposal.id().unwrap(), &rejection),
        Ok(())
    );
    assert_eq!(
        reject(
            &f.members[1],
            &f.vault,
            &proposal,
            &"x".repeat(MAX_REASON_BYTES + 1),
            &mut f.rng
        )
        .err(),
        Some(Error::ReasonTooLong(MAX_REASON_BYTES))
    );
}
