//! Byte-for-byte compatibility with Aztec 6.0.0: encodings, scalar multiplication, Poseidon2, and the account
//! derivation an Ome vault's address and decryption depend on. Expected values come from Aztec's own packages.
// Helpers outside `#[test]` unwrap like the tests do; `ivsk` and `ivpk` are Aztec's names.
#![allow(clippy::unwrap_used, clippy::similar_names)]

mod support;

use ome_crypto::{Base, Error, Point, Scalar, SecretScalar, aztec, poseidon2};
use support::vectors::{assert_point, aztec as vectors, base, bytes, point, scalar};

#[test]
fn generator_and_point_encodings_match_aztec() {
    let v = vectors();

    assert_point(&Point::generator(), &v["generator"]);
    assert_eq!(
        Point::from_bytes(&Point::generator().to_bytes()).unwrap(),
        Point::generator()
    );
}

#[test]
fn secret_and_public_scalar_multiplication_match_aztec() {
    // The secret path is the constant-time ladder; the edge scalars include q - 1 and a value just past q / 2.
    for case in vectors()["scalarMuls"].as_array().unwrap() {
        let k = scalar(&case["scalar"]);

        let public = Point::try_from(Point::generator().mul_public(&k)).unwrap();
        let secret = Point::generator_mul_secret(&SecretScalar::new(k)).unwrap();

        assert_point(&public, &case["result"]);
        assert_point(&secret, &case["result"]);
    }
}

#[test]
fn poseidon2_matches_aztec() {
    let v = vectors();
    let permutation = &v["poseidon2"]["permutation"];

    let state: [Base; 4] = core::array::from_fn(|i| base(&permutation["input"][i]));
    let expected: [Base; 4] = core::array::from_fn(|i| base(&permutation["output"][i]));
    assert_eq!(poseidon2::permutation(state), expected);

    for case in v["poseidon2"]["hashes"].as_array().unwrap() {
        let input: Vec<Base> = case["input"].as_array().unwrap().iter().map(base).collect();
        assert_eq!(
            poseidon2::hash(&input),
            base(&case["output"]),
            "{} inputs",
            input.len()
        );
    }

    for case in v["poseidon2"]["separated"].as_array().unwrap() {
        let separator = Base::from(case["separator"].as_u64().unwrap());
        let input: Vec<Base> = case["input"].as_array().unwrap().iter().map(base).collect();
        assert_eq!(
            poseidon2::hash_with_separator(separator, &input),
            base(&case["output"])
        );
    }
}

#[test]
fn account_derivation_matches_aztec_for_both_address_secret_signs() {
    let v = vectors();
    let accounts = v["accounts"].as_array().unwrap();
    let negated = accounts
        .iter()
        .filter(|a| a["addressSecretNegated"].as_bool().unwrap())
        .count();
    assert_eq!(negated, 3, "the vectors must cover both signs");

    for account in accounts {
        let ivsk = SecretScalar::new(scalar(&account["ivsk"]));
        let ivpk = Point::generator_mul_secret(&ivsk).unwrap();
        assert_point(&ivpk, &account["ivpk"]);

        let preaddress = aztec::preaddress(
            base(&account["publicKeysHash"]),
            base(&account["partialAddress"]),
        );
        assert_eq!(preaddress, base(&account["preaddress"]));

        let context = aztec::AddressContext::new(preaddress, &ivpk).unwrap();
        assert_eq!(context.address(), base(&account["address"]));
        assert_point(&context.address_point, &account["addressPoint"]);
        assert_eq!(
            context.sign == -Scalar::from(1u64),
            account["addressSecretNegated"].as_bool().unwrap()
        );

        let address_secret = aztec::single_key_address_secret(preaddress, &ivsk).unwrap();
        assert_eq!(*address_secret.expose(), scalar(&account["addressSecret"]));

        let shared = point(&account["ecdh"]["ephPk"])
            .mul_secret(&address_secret)
            .unwrap();
        assert_point(&shared, &account["ecdh"]["sharedPoint"]);
        assert_eq!(
            aztec::app_siloed_shared_secret(&shared, base(&account["ecdh"]["app"])),
            base(&account["ecdh"]["appSiloedSecret"])
        );
    }
}

#[test]
fn rejects_points_off_the_curve_and_non_canonical_coordinates() {
    // (1, 1) is not on y^2 = x^3 - 17.
    let mut off_curve = [0u8; 64];
    off_curve[31] = 1;
    off_curve[63] = 1;
    assert_eq!(Point::from_bytes(&off_curve), Err(Error::PointNotOnCurve));

    // A coordinate equal to the field modulus is not a canonical encoding.
    let modulus = bytes(vectors()["frModulus"].as_str().unwrap());
    let mut non_canonical = [0u8; 64];
    non_canonical[..32].copy_from_slice(&modulus);
    assert_eq!(
        Point::from_bytes(&non_canonical),
        Err(Error::NonCanonicalEncoding)
    );
}
