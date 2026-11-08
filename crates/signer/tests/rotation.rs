//! A rotation is signed only through `approve_rotation`, only for the rotation the member reviewed, and only if the new
//! committee holds the vault's key with fresh shares.

mod support;

use ome_protocol::{Error as ProtocolError, membership::Rotation, proposals::verify_approval};
use ome_signer::{Error, approve, approve_rotation};
use support::{NOW, chain, fixture, propose, rotation, transfer};

#[test]
fn a_reviewed_reshare_is_approved() {
    let mut f = fixture();
    let rotation = rotation(&mut f);
    let (proposal, payload) = propose(&f, vec![rotation.to_call(&f.vault)]);

    let approval = approve_rotation(
        &f.members[0],
        &f.vault,
        chain(),
        &proposal,
        &payload,
        &rotation,
        NOW,
        &mut f.rng,
    )
    .unwrap();

    assert_eq!(verify_approval(&f.vault, &proposal, &approval), Ok(()));
}

#[test]
fn a_rotation_cannot_be_approved_as_an_ordinary_proposal() {
    let mut f = fixture();
    let rotation = rotation(&mut f);
    let (proposal, payload) = propose(&f, vec![transfer(1), rotation.to_call(&f.vault)]);

    assert_eq!(
        approve(
            &f.members[0],
            &f.vault,
            chain(),
            &proposal,
            &payload,
            NOW,
            &mut f.rng
        )
        .err(),
        Some(Error::RotationNeedsReview)
    );
    assert_eq!(
        approve_rotation(
            &f.members[0],
            &f.vault,
            chain(),
            &proposal,
            &payload,
            &rotation,
            NOW,
            &mut f.rng
        )
        .err(),
        Some(Error::NotARotation)
    );
}

#[test]
fn only_the_reviewed_rotation_is_signed() {
    let mut f = fixture();
    let reviewed = rotation(&mut f);
    let performed = rotation(&mut f);
    let (proposal, payload) = propose(&f, vec![performed.to_call(&f.vault)]);

    assert_eq!(
        approve_rotation(
            &f.members[1],
            &f.vault,
            chain(),
            &proposal,
            &payload,
            &reviewed,
            NOW,
            &mut f.rng
        )
        .err(),
        Some(Error::RotationMismatch)
    );
}

#[test]
fn a_rotation_that_keeps_the_old_shares_is_not_signed() {
    let mut f = fixture();
    let fresh = rotation(&mut f);
    let stale = Rotation {
        committee: f.vault.committee().clone(),
        ..fresh
    };
    let (proposal, payload) = propose(&f, vec![stale.to_call(&f.vault)]);

    assert_eq!(
        approve_rotation(
            &f.members[1],
            &f.vault,
            chain(),
            &proposal,
            &payload,
            &stale,
            NOW,
            &mut f.rng
        )
        .err(),
        Some(Error::Protocol(ProtocolError::SharesNotRefreshed))
    );
}
