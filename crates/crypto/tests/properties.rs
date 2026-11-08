//! Property tests over random thresholds, committees, subsets and inputs. `PROPTEST_CASES` sets the case count
//! (64 locally, 1024 on main in CI).
// Helpers outside `#[test]` unwrap like the tests do; `ivsk` and `ivpk` are Aztec's names.
#![allow(clippy::unwrap_used, clippy::similar_names)]

mod support;

use ark_ff::PrimeField;
use ome_crypto::{
    Base, Point, Scalar, SecretScalar, aztec, curve,
    threshold::{
        dkg::KeyShare,
        ecdh::{AddressSecretShare, EcdhContext, Partial, combine},
        shamir::{Polynomial, lagrange_coefficient},
    },
};
use proptest::prelude::*;
use support::{
    ceremony::{random_point, random_subset, run_dkg, run_reshare, threshold_ecdh},
    seeded,
};

proptest! {
    #[test]
    fn lagrange_weights_at_zero_sum_to_one(t in 1usize..=8, extra in 0u32..=4, seed: u64) {
        let set = random_subset(u32::try_from(t).unwrap() + extra, t, seed);

        let total: Scalar = set.iter().map(|&i| lagrange_coefficient(i, &set).unwrap()).sum();

        prop_assert_eq!(total, Scalar::from(1u64));
    }

    #[test]
    fn any_threshold_subset_reconstructs_the_secret(t in 1usize..=6, extra in 0u32..=3, seed: u64) {
        let mut rng = seeded(seed);
        let polynomial = Polynomial::random(SecretScalar::random(&mut rng), t, &mut rng).unwrap();
        let set = random_subset(u32::try_from(t).unwrap() + extra, t, seed ^ 1);

        let value: Scalar = set
            .iter()
            .map(|&i| lagrange_coefficient(i, &set).unwrap() * polynomial.evaluate(i).unwrap().expose())
            .sum();

        prop_assert_eq!(value, *polynomial.constant().expose());
    }

    #[test]
    fn threshold_ecdh_equals_single_key_ecdh(t in 2usize..=5, extra in 0u32..=3, seed: u64, preaddress: u64) {
        let mut rng = seeded(seed);
        let ivsk = SecretScalar::random(&mut rng);
        let ivpk = Point::generator_mul_secret(&ivsk).unwrap();
        let preaddress = Base::from(preaddress);
        let address = aztec::AddressContext::new(preaddress, &ivpk).unwrap();
        let polynomial = Polynomial::random(SecretScalar::new(*ivsk.expose()), t, &mut rng).unwrap();
        let input = random_point(&mut rng);
        let context = EcdhContext { vault: address.address(), epoch: 1 };

        let partials: Vec<Partial> = random_subset(u32::try_from(t).unwrap() + extra, t, seed ^ 2)
            .into_iter()
            .map(|i| {
                let share = AddressSecretShare::new(i, &polynomial.evaluate(i).unwrap(), &address).unwrap();
                share.partial(&input, &context, &mut rng).unwrap()
            })
            .collect();
        let address_secret = aztec::single_key_address_secret(preaddress, &ivsk).unwrap();

        prop_assert_eq!(combine(&partials, t).unwrap(), input.mul_secret(&address_secret).unwrap());
    }

    #[test]
    fn rotation_preserves_the_group_key_and_decryption(
        t in 2usize..=4,
        extra in 0u32..=2,
        removed in 0usize..=3,
        joined in 0u32..=2,
        new_t in 1usize..=4,
        seed: u64,
    ) {
        let n = u32::try_from(t).unwrap() + extra;
        let removed = removed.min(t - 1).min(n as usize - t);
        let mut rng = seeded(seed);
        let old = run_dkg(t, n, &mut rng);
        let address = aztec::AddressContext::new(Base::from(seed), old[0].group_public_key()).unwrap();
        let input = random_point(&mut rng);
        let before = threshold_ecdh(&old.iter().take(t).collect::<Vec<_>>(), &address, &input, &mut rng);

        // At most t - 1 members leave, so at least t stay; t of the stayers deal.
        let stayers = random_subset(n, n as usize - removed, seed ^ 3);
        let new_n = u32::try_from(stayers.len()).unwrap() + joined;
        let new_t = new_t.min(new_n as usize);
        let parameters = support::ceremony::reshare_parameters(stayers[..t].to_vec(), t, new_n, new_t);
        let new = run_reshare(&old, &parameters, &mut rng);

        prop_assert_eq!(new[0].group_public_key(), old[0].group_public_key());
        let quorum: Vec<&KeyShare> =
            random_subset(new_n, new_t, seed ^ 4).into_iter().map(|j| &new[j as usize - 1]).collect();
        prop_assert_eq!(threshold_ecdh(&quorum, &address, &input, &mut rng), before);
    }

    #[test]
    fn the_constant_time_ladder_matches_arkworks(seed: u64, k_bytes: [u8; 32]) {
        let base_point = random_point(&mut seeded(seed));
        let k = Scalar::from_le_bytes_mod_order(&k_bytes);

        let secret = base_point.mul_secret(&SecretScalar::new(k));

        prop_assert_eq!(secret, Point::try_from(base_point.mul_public(&k)));
    }

    #[test]
    fn decoders_never_panic_and_accepted_input_round_trips(bytes in proptest::collection::vec(any::<u8>(), 0..80)) {
        if let Ok(array) = <[u8; 64]>::try_from(bytes.as_slice())
            && let Ok(point) = Point::from_bytes(&array)
        {
            prop_assert_eq!(point.to_bytes(), array);
        }
        if let Ok(value) = curve::scalar_from_bytes(&bytes) {
            prop_assert_eq!(curve::scalar_to_bytes(&value).to_vec(), bytes.clone());
        }
        if let Ok(value) = curve::base_from_bytes(&bytes) {
            prop_assert_eq!(curve::base_to_bytes(&value).to_vec(), bytes);
        }
    }
}
