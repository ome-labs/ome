//! A rotation is approved only if the contract would accept it and the new committee really holds the vault's viewing
//! key with fresh shares.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{Base, Point, SecretScalar};
use ome_protocol::{
    Error,
    membership::{Rotation, check_rotation, is_rotation_call, verify_shares},
    payload::selector,
    vault::{
        Committee, SignerSet,
        constants::{ROTATE_MEMBERSHIP_SELECTOR, ROTATE_MEMBERSHIP_SIGNATURE},
    },
};
use ome_wire::types::field;
use support::fixture::{fixture, reshare_to, run_dkg};

/// The signer keys after member 3 leaves and a newcomer joins in their slot.
fn replaced_signers(f: &mut support::fixture::Fixture) -> SignerSet {
    let newcomer = Point::generator_mul_secret(&SecretScalar::random(&mut f.rng)).unwrap();
    let mut keys = f.vault.signers().keys().to_vec();
    keys[2] = newcomer;
    SignerSet::new(keys).unwrap()
}

fn committee_of(shares: &[ome_crypto::threshold::dkg::KeyShare], t: u32) -> Committee {
    Committee::new(
        t,
        shares
            .iter()
            .map(|s| s.verification_share(s.index()).unwrap())
            .collect(),
    )
    .unwrap()
}

#[test]
fn a_real_reshare_passes_and_bumps_the_version() {
    let mut f = fixture();
    let signers = replaced_signers(&mut f);
    let reshared = reshare_to(&f.shares, vec![1, 2], 3, 2, &mut f.rng);
    let rotation = Rotation {
        signers,
        threshold: 2,
        committee: committee_of(&reshared, 2),
    };

    let next = check_rotation(&f.vault, &rotation).unwrap();

    assert_eq!(next.version, f.config.version + 1);
    assert_eq!(next.committee_commitment, rotation.committee.commitment());
}

#[test]
fn shares_must_lie_on_one_polynomial_through_the_vault_key() {
    let mut f = fixture();
    let random: Vec<Point> = (0..3)
        .map(|_| Point::generator_mul_secret(&SecretScalar::random(&mut f.rng)).unwrap())
        .collect();
    let other_key = run_dkg(2, 3, &mut f.rng);

    assert_eq!(
        verify_shares(f.vault.group_key(), &Committee::new(2, random).unwrap()),
        Err(Error::ShareOffPolynomial(3))
    );
    assert_eq!(
        verify_shares(f.vault.group_key(), &committee_of(&other_key, 2)),
        Err(Error::GroupKeyMismatch)
    );
}

#[test]
fn a_rotation_that_keeps_the_old_shares_is_refused() {
    let mut f = fixture();
    let signers = replaced_signers(&mut f);
    let rotation = Rotation {
        signers,
        threshold: 2,
        committee: f.vault.committee().clone(),
    };

    assert_eq!(
        check_rotation(&f.vault, &rotation),
        Err(Error::SharesNotRefreshed)
    );
}

#[test]
fn the_contract_rules_are_applied_first() {
    let mut f = fixture();
    let reshared = reshare_to(&f.shares, vec![1, 2], 2, 2, &mut f.rng);
    let only_first = SignerSet::new(vec![
        f.vault.signers().keys()[0],
        replaced_signers(&mut f).keys()[2],
    ])
    .unwrap();
    let too_many_left = Rotation {
        signers: only_first,
        threshold: 1,
        committee: committee_of(&reshared, 2),
    };

    assert_eq!(
        check_rotation(&f.vault, &too_many_left),
        Err(Error::TooManyRemoved {
            removed: 2,
            allowed: 1
        })
    );

    let signers = replaced_signers(&mut f);
    let reshared = reshare_to(&f.shares, vec![1, 2], 3, 2, &mut f.rng);
    let high = Rotation {
        signers,
        threshold: 4,
        committee: committee_of(&reshared, 2),
    };
    assert_eq!(
        check_rotation(&f.vault, &high),
        Err(Error::Threshold {
            threshold: 4,
            members: 3
        })
    );
}

#[test]
fn a_rotation_round_trips_through_its_rotate_membership_call() {
    let mut f = fixture();
    let signers = replaced_signers(&mut f);
    let reshared = reshare_to(&f.shares, vec![1, 2], 3, 2, &mut f.rng);
    let rotation = Rotation {
        signers,
        threshold: 2,
        committee: committee_of(&reshared, 2),
    };

    let call = rotation.to_call(&f.vault);

    assert_eq!(
        selector(ROTATE_MEMBERSHIP_SIGNATURE),
        Ok(ROTATE_MEMBERSHIP_SELECTOR)
    );
    assert_eq!(call.args.len(), 51);
    assert!(is_rotation_call(&call, &f.vault));
    assert_eq!(Rotation::from_call(&call, &f.vault), Ok(rotation));
}

#[test]
fn a_rotate_membership_call_that_is_not_a_rotation_of_this_vault_is_refused() {
    let mut f = fixture();
    let signers = replaced_signers(&mut f);
    let reshared = reshare_to(&f.shares, vec![1, 2], 3, 2, &mut f.rng);
    let call = Rotation {
        signers,
        threshold: 2,
        committee: committee_of(&reshared, 2),
    }
    .to_call(&f.vault);
    let refused = |change: fn(&mut ome_protocol::payload::Call), reason| {
        let mut bad = call.clone();
        change(&mut bad);
        assert_eq!(
            Rotation::from_call(&bad, &f.vault),
            Err(Error::InvalidRotationCall(reason))
        );
    };

    refused(
        |c| c.to = field(&Base::from(1u64)),
        "not rotate_membership on this vault",
    );
    refused(|c| c.args.truncate(50), "wrong argument count");
    refused(
        |c| c.args[0] = field(&Base::from(5u64)),
        "current keys are not the vault's signer set",
    );
    refused(
        |c| c.args[30] = field(&Base::from(5u64)),
        "non-empty slot past the member count",
    );
    refused(
        |c| c.args[32] = field(&Base::from(1u64 << 32)),
        "count or threshold above u32",
    );
}
