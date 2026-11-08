//! Shamir sharing and Feldman commitments: the arithmetic every ceremony relies on.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{
    Error, Scalar, SecretScalar,
    threshold::shamir::{
        Commitments, MemberIndex, Polynomial, lagrange_coefficient, validate_index_set,
    },
};
use support::seeded;

fn interpolate(shares: &[(MemberIndex, &SecretScalar)]) -> Scalar {
    let set: Vec<MemberIndex> = shares.iter().map(|(i, _)| *i).collect();
    shares
        .iter()
        .map(|(i, s)| lagrange_coefficient(*i, &set).unwrap() * s.expose())
        .sum()
}

#[test]
fn any_threshold_subset_reconstructs_and_fewer_do_not() {
    let mut rng = seeded(1);
    let polynomial = Polynomial::random(SecretScalar::random(&mut rng), 3, &mut rng).unwrap();
    let shares: Vec<SecretScalar> = (1..=5).map(|i| polynomial.evaluate(i).unwrap()).collect();
    let secret = *polynomial.constant().expose();

    for subset in [[1, 2, 3], [1, 3, 5], [2, 4, 5]] {
        let chosen: Vec<_> = subset
            .iter()
            .map(|&i| (i, &shares[i as usize - 1]))
            .collect();
        assert_eq!(interpolate(&chosen), secret, "subset {subset:?}");
    }
    assert_ne!(interpolate(&[(1, &shares[0]), (2, &shares[1])]), secret);
}

#[test]
fn commitments_verify_exactly_the_dealt_shares() {
    let mut rng = seeded(2);
    let polynomial = Polynomial::random(SecretScalar::random(&mut rng), 3, &mut rng).unwrap();
    let commitments = polynomial.commitments().unwrap();
    assert_eq!(commitments.threshold(), 3);

    for i in 1..=4 {
        assert!(commitments.verify_share(i, &polynomial.evaluate(i).unwrap()));
    }
    let share = polynomial.evaluate(1).unwrap();
    assert!(!commitments.verify_share(2, &share));
    assert!(!commitments.verify_share(1, &SecretScalar::new(*share.expose() + Scalar::from(1u64))));
}

#[test]
fn verification_shares_interpolate_back_to_the_commitments() {
    let mut rng = seeded(3);
    let polynomial = Polynomial::random(SecretScalar::random(&mut rng), 3, &mut rng).unwrap();
    let commitments = polynomial.commitments().unwrap();
    let evaluations: Vec<_> = [5, 2, 4, 1]
        .iter()
        .map(|&i| (i, commitments.evaluate(i).unwrap()))
        .collect();

    assert_eq!(
        Commitments::interpolate(3, &evaluations).unwrap(),
        commitments
    );

    // A fourth point off the degree-2 polynomial is named.
    let mut off = evaluations.clone();
    off[3].1 = commitments.evaluate(7).unwrap();
    assert_eq!(
        Commitments::interpolate(3, &off),
        Err(Error::InvalidShare(1))
    );
    assert!(matches!(
        Commitments::interpolate(3, &evaluations[..2]),
        Err(Error::InsufficientShares {
            needed: 3,
            actual: 2
        })
    ));
}

#[test]
fn rejects_zero_duplicate_and_foreign_indices() {
    let mut rng = seeded(4);

    assert_eq!(validate_index_set(&[1, 2, 0]), Err(Error::InvalidIndex));
    assert_eq!(
        validate_index_set(&[1, 2, 2]),
        Err(Error::DuplicateIndex(2))
    );
    assert_eq!(
        lagrange_coefficient(4, &[1, 2, 3]),
        Err(Error::InvalidIndex)
    );

    assert!(Polynomial::random(SecretScalar::random(&mut rng), 0, &mut rng).is_err());
    let polynomial = Polynomial::random(SecretScalar::random(&mut rng), 1, &mut rng).unwrap();
    assert_eq!(polynomial.evaluate(0).err(), Some(Error::InvalidIndex));
}
