//! Feldman DKG: honest ceremonies agree on one group key, and every failure path names the dealer at fault.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{
    Base, Error, Point, Scalar, SecretScalar,
    aztec::{self, AddressContext},
    threshold::{
        dkg::{self, KeyShare, Parameters, complaint_is_valid},
        shamir::{Commitments, MemberIndex, lagrange_coefficient},
    },
};
use support::{
    ceremony::{dkg_parameters, dkg_rounds, random_point, run_dkg, threshold_ecdh},
    seeded,
};

/// Reconstructs the group secret from shares. Test-only: no Ome component ever does this.
fn reconstruct(shares: &[&KeyShare]) -> SecretScalar {
    let set: Vec<MemberIndex> = shares.iter().map(|s| s.index()).collect();
    SecretScalar::new(
        shares
            .iter()
            .map(|s| lagrange_coefficient(s.index(), &set).unwrap() * s.share().expose())
            .sum(),
    )
}

#[test]
fn members_agree_on_the_group_key_and_their_verification_shares() {
    let mut rng = seeded(11);

    for (t, n) in [(1, 1), (2, 3), (3, 5), (8, 8)] {
        let shares = run_dkg(t, n, &mut rng);
        let first = &shares[0];

        for share in &shares {
            assert_eq!(share.group_public_key(), first.group_public_key());
            assert_eq!(share.transcript(), first.transcript());
            assert_eq!(
                share.verification_shares().unwrap(),
                first.verification_shares().unwrap()
            );
            assert_eq!(
                Point::generator_mul_secret(share.share()).unwrap(),
                share.verification_share(share.index()).unwrap()
            );
        }

        let quorum: Vec<&KeyShare> = shares.iter().take(t).collect();
        let secret = reconstruct(&quorum);
        assert_eq!(
            Point::generator_mul_secret(&secret).unwrap(),
            *first.group_public_key(),
            "{t}-of-{n}"
        );
    }
}

#[test]
fn dkg_shares_decrypt_like_the_single_key_account() {
    let mut rng = seeded(12);
    let shares = run_dkg(2, 3, &mut rng);
    let preaddress = Base::from(0xabcd_u64);
    let context = AddressContext::new(preaddress, shares[0].group_public_key()).unwrap();
    let input = random_point(&mut rng);

    let ivsk = reconstruct(&[&shares[0], &shares[2]]);
    let address_secret = aztec::single_key_address_secret(preaddress, &ivsk).unwrap();
    let expected = input.mul_secret(&address_secret).unwrap();

    for [a, b] in [[0, 1], [0, 2], [1, 2]] {
        assert_eq!(
            threshold_ecdh(&[&shares[a], &shares[b]], &context, &input, &mut rng),
            expected
        );
    }
}

#[test]
fn a_bad_proof_of_knowledge_names_the_dealer() {
    let mut rng = seeded(13);
    let parameters = dkg_parameters(2, 3);
    let (secrets, mut round1, _) = dkg_rounds(&parameters, &mut rng);

    round1[1].proof.response += Scalar::from(1u64);

    assert_eq!(
        dkg::round2(&parameters, &secrets[0], &round1).err(),
        Some(Error::InvalidProof(2))
    );
}

#[test]
fn proofs_do_not_carry_over_to_another_session() {
    let mut rng = seeded(14);
    let parameters = dkg_parameters(2, 3);
    let (secrets, round1, _) = dkg_rounds(&parameters, &mut rng);
    let other = Parameters {
        session: [8u8; 32],
        ..parameters
    };

    assert!(matches!(
        dkg::round2(&other, &secrets[0], &round1),
        Err(Error::InvalidProof(_))
    ));
}

#[test]
fn commitments_of_the_wrong_degree_name_the_dealer() {
    let mut rng = seeded(15);
    let parameters = dkg_parameters(2, 3);
    let (secrets, mut round1, _) = dkg_rounds(&parameters, &mut rng);
    let (_, wider) = dkg::round1(&dkg_parameters(3, 3), 3, &mut rng).unwrap();

    round1[2].commitments = wider.commitments;

    assert_eq!(
        dkg::round2(&parameters, &secrets[0], &round1).err(),
        Some(Error::InvalidCommitments(3))
    );
}

#[test]
fn a_share_off_its_commitments_names_the_dealer_and_the_complaint_holds() {
    let mut rng = seeded(16);
    let parameters = dkg_parameters(2, 3);
    let (mut secrets, round1, mut round2) = dkg_rounds(&parameters, &mut rng);

    let tampered = round2
        .iter_mut()
        .find(|p| p.dealer == 3 && p.recipient == 1)
        .unwrap();
    tampered.share = SecretScalar::new(*tampered.share.expose() + Scalar::from(1u64));
    let evidence = SecretScalar::new(*tampered.share.expose());

    let member_1 = secrets.remove(0);
    assert_eq!(
        dkg::finalize(&parameters, member_1, &round1, &round2).err(),
        Some(Error::InvalidShare(3))
    );
    assert!(complaint_is_valid(&round1[2], 1, &evidence));

    let honest = round2
        .iter()
        .find(|p| p.dealer == 2 && p.recipient == 1)
        .unwrap();
    assert!(!complaint_is_valid(&round1[1], 1, &honest.share));
}

#[test]
fn missing_packages_and_bad_parameters_are_refused() {
    let mut rng = seeded(17);
    let parameters = dkg_parameters(2, 3);
    let (mut secrets, round1, round2) = dkg_rounds(&parameters, &mut rng);

    assert!(matches!(
        dkg::round2(&parameters, &secrets[0], &round1[..2]),
        Err(Error::InsufficientShares {
            needed: 3,
            actual: 2
        })
    ));
    let without_dealer_2: Vec<_> = round2.into_iter().filter(|p| p.dealer != 2).collect();
    assert!(matches!(
        dkg::finalize(&parameters, secrets.remove(0), &round1, &without_dealer_2),
        Err(Error::InsufficientShares { .. })
    ));

    assert_eq!(
        dkg::round1(&dkg_parameters(4, 3), 1, &mut rng).err(),
        Some(Error::InvalidThreshold)
    );
    assert_eq!(
        dkg::round1(&dkg_parameters(0, 3), 1, &mut rng).err(),
        Some(Error::InvalidThreshold)
    );
    assert_eq!(
        dkg::round1(&parameters, 9, &mut rng).err(),
        Some(Error::InvalidIndex)
    );
}

#[test]
fn a_stored_share_restores_only_with_its_own_secret() {
    let mut rng = seeded(18);
    let shares = run_dkg(2, 3, &mut rng);
    let share = &shares[1];
    let restore = |secret: &SecretScalar| {
        KeyShare::restore(
            share.index(),
            share.threshold(),
            share.participants().to_vec(),
            SecretScalar::new(*secret.expose()),
            share.group_commitments().clone(),
            *share.transcript(),
        )
    };

    assert_eq!(
        restore(share.share()).unwrap().group_public_key(),
        share.group_public_key()
    );
    assert!(matches!(
        restore(shares[0].share()),
        Err(Error::InvalidShare(2))
    ));
}

#[test]
fn published_verification_shares_rebuild_the_group_commitments() {
    let mut rng = seeded(19);
    let shares = run_dkg(2, 3, &mut rng);
    let share = &shares[0];
    let evaluations: Vec<_> = share
        .participants()
        .iter()
        .map(|&i| (i, share.verification_share(i).unwrap()))
        .collect();

    assert_eq!(
        &Commitments::interpolate(2, &evaluations).unwrap(),
        share.group_commitments()
    );
}
