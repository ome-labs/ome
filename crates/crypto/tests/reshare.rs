//! Resharing: a new committee keeps the vault's key and decryption output, a removed member's share no longer
//! combines, and a dishonest dealer is named.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{
    Base, Error, Scalar, SecretScalar,
    aztec::AddressContext,
    threshold::{
        ecdh::{AddressSecretShare, EcdhContext, combine},
        reshare::{self, SubShare},
    },
};
use support::{
    ceremony::{random_point, reshare_parameters, run_dkg, run_reshare, threshold_ecdh},
    seeded,
};

#[test]
fn replacing_a_member_keeps_the_key_and_locks_out_the_leaver() {
    let mut rng = seeded(21);
    let old = run_dkg(2, 3, &mut rng);
    let address = AddressContext::new(Base::from(77u64), old[0].group_public_key()).unwrap();
    let input = random_point(&mut rng);
    let before = threshold_ecdh(&[&old[0], &old[1]], &address, &input, &mut rng);

    // Members 1 and 2 stay and deal; member 3 leaves and a newcomer takes index 3.
    let new = run_reshare(&old, &reshare_parameters(vec![1, 2], 2, 3, 2), &mut rng);

    for share in &new {
        assert_eq!(share.group_public_key(), old[0].group_public_key());
        assert_eq!(share.transcript(), new[0].transcript());
    }
    for [a, b] in [[0, 1], [0, 2], [1, 2]] {
        assert_eq!(
            threshold_ecdh(&[&new[a], &new[b]], &address, &input, &mut rng),
            before
        );
    }

    let context = EcdhContext {
        vault: address.address(),
        epoch: 0,
    };
    let partial = |index, share: &SecretScalar, rng: &mut _| {
        AddressSecretShare::new(index, share, &address)
            .unwrap()
            .partial(&input, &context, rng)
            .unwrap()
    };
    let leaver = partial(3, old[2].share(), &mut rng);
    let stayer = partial(1, new[0].share(), &mut rng);
    assert_ne!(combine(&[stayer, leaver], 2).unwrap(), before);
}

#[test]
fn the_threshold_can_grow_and_shrink() {
    let mut rng = seeded(22);
    let input = random_point(&mut rng);
    let old = run_dkg(2, 3, &mut rng);
    let address = AddressContext::new(Base::from(5u64), old[0].group_public_key()).unwrap();
    let before = threshold_ecdh(&[&old[1], &old[2]], &address, &input, &mut rng);

    let grown = run_reshare(&old, &reshare_parameters(vec![2, 3], 2, 4, 3), &mut rng);
    assert_eq!(
        threshold_ecdh(
            &[&grown[0], &grown[2], &grown[3]],
            &address,
            &input,
            &mut rng
        ),
        before
    );

    let shrunk = run_reshare(
        &grown,
        &reshare_parameters(vec![1, 2, 4], 3, 2, 2),
        &mut rng,
    );
    assert_eq!(
        threshold_ecdh(&[&shrunk[0], &shrunk[1]], &address, &input, &mut rng),
        before
    );
}

#[test]
fn new_members_agree_on_fresh_verification_shares() {
    let mut rng = seeded(23);
    let old = run_dkg(2, 3, &mut rng);

    let new = run_reshare(&old, &reshare_parameters(vec![1, 3], 2, 3, 2), &mut rng);

    let shares = new[0].verification_shares().unwrap();
    assert!(
        new.iter()
            .all(|s| s.verification_shares().unwrap() == shares)
    );
    assert_eq!(
        new[0].group_commitments().constant_term(),
        old[0].group_commitments().constant_term()
    );
    assert_ne!(shares, old[0].verification_shares().unwrap());
}

#[test]
fn a_dealer_resharing_the_wrong_secret_is_named() {
    let mut rng = seeded(24);
    let old = run_dkg(2, 3, &mut rng);
    let parameters = reshare_parameters(vec![1, 2], 2, 3, 2);
    let (mut dishonest, _) = reshare::deal(&parameters, &old[0], &mut rng).unwrap();
    let (other, _) = reshare::deal(&parameters, &old[1], &mut rng).unwrap();

    dishonest.commitments = other.commitments;

    assert_eq!(
        reshare::verify_dealer(&parameters, old[0].group_commitments(), &dishonest),
        Err(Error::InvalidProof(1))
    );
}

#[test]
fn a_sub_share_off_its_commitments_names_the_dealer() {
    let mut rng = seeded(25);
    let old = run_dkg(2, 3, &mut rng);
    let parameters = reshare_parameters(vec![1, 2], 2, 3, 2);
    let (first, first_subs) = reshare::deal(&parameters, &old[0], &mut rng).unwrap();
    let (second, second_subs) = reshare::deal(&parameters, &old[1], &mut rng).unwrap();

    let mut received: Vec<SubShare> = first_subs
        .into_iter()
        .chain(second_subs)
        .filter(|s| s.recipient == 1)
        .collect();
    received[1].share = SecretScalar::new(*received[1].share.expose() + Scalar::from(1u64));

    assert_eq!(
        reshare::receive(
            &parameters,
            old[0].group_commitments(),
            1,
            &[first, second],
            &received
        )
        .err(),
        Some(Error::InvalidShare(2))
    );
}

#[test]
fn rejects_too_few_dealers_a_non_dealer_and_an_impossible_threshold() {
    let mut rng = seeded(26);
    let old = run_dkg(2, 3, &mut rng);

    assert_eq!(
        reshare::deal(&reshare_parameters(vec![1], 2, 3, 2), &old[0], &mut rng).err(),
        Some(Error::InsufficientShares {
            needed: 2,
            actual: 1
        })
    );
    assert_eq!(
        reshare::deal(&reshare_parameters(vec![1, 2], 2, 3, 2), &old[2], &mut rng).err(),
        Some(Error::InvalidIndex)
    );
    assert_eq!(
        reshare::deal(&reshare_parameters(vec![1, 2], 2, 3, 4), &old[0], &mut rng).err(),
        Some(Error::InvalidThreshold)
    );
}
