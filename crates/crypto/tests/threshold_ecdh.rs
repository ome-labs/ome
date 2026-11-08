//! Threshold ECDH: shares of real Aztec accounts reproduce Aztec's shared secret, and DLEQ proofs reject every
//! forged, relabelled or replayed partial.
// Helpers outside `#[test]` unwrap like the tests do; `ivsk` and `ivpk` are Aztec's names.
#![allow(clippy::unwrap_used, clippy::similar_names)]

mod support;

use ome_crypto::{
    Base, Error, Point, SecretScalar,
    threshold::{
        ecdh::{
            AddressSecretShare, EcdhContext, Partial, combine, verify_and_combine, verify_partial,
        },
        shamir::{MemberIndex, Polynomial},
    },
};
use rand_chacha::ChaCha20Rng;
use support::{
    ceremony::{deal_address_shares, random_point, subsets},
    seeded,
    vectors::{aztec as vectors, base, point, scalar},
};

struct Setup {
    shares: Vec<AddressSecretShare>,
    input: Point,
    context: EcdhContext,
    rng: ChaCha20Rng,
}

/// A 2-of-3 committee over a random key, one ECDH input, and the context partials are bound to.
fn setup() -> Setup {
    let mut rng = seeded(3);
    let ivsk = SecretScalar::random(&mut rng);
    let (shares, address) = deal_address_shares(&ivsk, Base::from(1234u64), 2, 3, &mut rng);
    let input = random_point(&mut rng);
    Setup {
        shares,
        input,
        context: EcdhContext {
            vault: address.address(),
            epoch: 1,
        },
        rng,
    }
}

#[test]
fn every_qualified_subset_reproduces_aztecs_shared_secret() {
    let v = vectors();
    let mut rng = seeded(1);

    for (t, n) in [(2, 3), (3, 5)] {
        for account in v["accounts"].as_array().unwrap() {
            let ivsk = SecretScalar::new(scalar(&account["ivsk"]));
            let (shares, address) =
                deal_address_shares(&ivsk, base(&account["preaddress"]), t, n, &mut rng);
            let eph_pk = point(&account["ecdh"]["ephPk"]);
            let context = EcdhContext {
                vault: address.address(),
                epoch: 7,
            };

            for set in subsets(n, t) {
                let partials: Vec<Partial> = set
                    .iter()
                    .map(|&i| {
                        let share = &shares[i as usize - 1];
                        let partial = share.partial(&eph_pk, &context, &mut rng).unwrap();
                        verify_partial(&partial, &eph_pk, share.public(), &context).unwrap();
                        partial
                    })
                    .collect();

                assert_eq!(
                    combine(&partials, t).unwrap(),
                    point(&account["ecdh"]["sharedPoint"]),
                    "{set:?}"
                );
            }
        }
    }
}

#[test]
fn member_public_shares_follow_from_the_viewing_commitments() {
    let mut rng = seeded(2);
    let ivsk = SecretScalar::random(&mut rng);
    let ivpk = Point::generator_mul_secret(&ivsk).unwrap();
    let address = ome_crypto::aztec::AddressContext::new(Base::from(99u64), &ivpk).unwrap();
    let polynomial = Polynomial::random(ivsk, 2, &mut rng).unwrap();
    let commitments = polynomial.commitments().unwrap();

    for i in 1..=3 {
        let share = AddressSecretShare::new(i, &polynomial.evaluate(i).unwrap(), &address).unwrap();
        let verification = commitments.evaluate(i).unwrap();
        assert_eq!(
            address.address_verification_share(&verification).unwrap(),
            *share.public()
        );
    }
}

#[test]
fn a_forged_partial_is_rejected() {
    let Setup {
        shares,
        input,
        context,
        mut rng,
    } = setup();
    let mut partial = shares[0].partial(&input, &context, &mut rng).unwrap();

    partial.point = Point::generator();

    assert_eq!(
        verify_partial(&partial, &input, shares[0].public(), &context),
        Err(Error::InvalidProof(1))
    );
}

#[test]
fn a_partial_is_bound_to_its_member_vault_epoch_and_input() {
    let Setup {
        shares,
        input,
        context,
        mut rng,
    } = setup();
    let partial = shares[0].partial(&input, &context, &mut rng).unwrap();
    let public = shares[0].public();

    assert!(verify_partial(&partial, &input, shares[1].public(), &context).is_err());
    assert!(
        verify_partial(
            &partial,
            &input,
            public,
            &EcdhContext {
                epoch: 2,
                ..context
            }
        )
        .is_err()
    );
    assert!(
        verify_partial(
            &partial,
            &input,
            public,
            &EcdhContext {
                vault: Base::from(5u64),
                ..context
            }
        )
        .is_err()
    );
    assert!(verify_partial(&partial, &Point::generator(), public, &context).is_err());

    let relabelled = Partial {
        index: 2,
        ..partial
    };
    assert!(verify_partial(&relabelled, &input, shares[1].public(), &context).is_err());
}

#[test]
fn verify_and_combine_names_bad_partials_and_needs_the_threshold() {
    let Setup {
        shares,
        input,
        context,
        mut rng,
    } = setup();
    let publics: Vec<Point> = shares.iter().map(|s| *s.public()).collect();
    let lookup = |i: MemberIndex| publics.get((i as usize).checked_sub(1)?).copied();

    let mut forged = shares[0].partial(&input, &context, &mut rng).unwrap();
    forged.point = Point::generator();
    let second = shares[1].partial(&input, &context, &mut rng).unwrap();
    let third = shares[2].partial(&input, &context, &mut rng).unwrap();

    let (result, invalid) =
        verify_and_combine(&[forged, second, third], &input, lookup, 2, &context).unwrap();
    assert_eq!(invalid, vec![1]);
    assert_eq!(result, combine(&[second, third], 2).unwrap());

    assert_eq!(
        verify_and_combine(&[forged, second], &input, lookup, 2, &context).err(),
        Some(Error::InsufficientShares {
            needed: 2,
            actual: 1
        })
    );
}

#[test]
fn combine_refuses_too_few_or_repeated_partials() {
    let Setup {
        shares,
        input,
        context,
        mut rng,
    } = setup();
    let second = shares[1].partial(&input, &context, &mut rng).unwrap();

    assert_eq!(
        combine(&[second], 2).err(),
        Some(Error::InsufficientShares {
            needed: 2,
            actual: 1
        })
    );
    assert_eq!(
        combine(&[second, second], 2).err(),
        Some(Error::DuplicateIndex(2))
    );
}
