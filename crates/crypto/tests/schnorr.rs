//! Aztec's Grumpkin Schnorr signatures: Aztec-made signatures verify here, Ome-made signatures verify by the same rule,
//! and the limb encoding the vault contract reads round-trips.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{
    Base, Point, SecretScalar,
    schnorr::{self, Signature},
};
use support::{
    seeded,
    vectors::{aztec as vectors, base, bytes, point},
};

fn signature(case: &serde_json::Value) -> Signature {
    let encoded: [u8; 64] = bytes(case["signature"].as_str().unwrap())
        .try_into()
        .unwrap();
    Signature::from_bytes(&encoded).unwrap()
}

#[test]
fn aztec_signatures_verify_and_tampered_messages_do_not() {
    for case in vectors()["schnorr"].as_array().unwrap() {
        let public_key = point(&case["publicKey"]);
        let signature = signature(case);

        assert!(schnorr::verify(
            &public_key,
            base(&case["message"]),
            &signature
        ));
        assert!(!schnorr::verify(
            &public_key,
            base(&case["tamperedMessage"]),
            &signature
        ));
    }
}

#[test]
fn signature_bytes_and_contract_limbs_round_trip() {
    for case in vectors()["schnorr"].as_array().unwrap() {
        let signature = signature(case);
        let limbs: [Base; 4] = core::array::from_fn(|i| base(&case["limbs"][i]));

        assert_eq!(
            signature.to_bytes().to_vec(),
            bytes(case["signature"].as_str().unwrap())
        );
        assert_eq!(signature.to_limbs(), limbs);
        assert_eq!(Signature::from_limbs(&limbs).unwrap(), signature);
    }
}

#[test]
fn rejects_another_key_and_a_zero_challenge() {
    let v = vectors();
    let case = &v["schnorr"][0];
    let signature = signature(case);
    let message = base(&case["message"]);

    assert!(!schnorr::verify(
        &point(&v["schnorr"][1]["publicKey"]),
        message,
        &signature
    ));
    assert!(!schnorr::verify(
        &point(&case["publicKey"]),
        message,
        &Signature {
            e: Base::from(0u64),
            ..signature
        }
    ));
}

#[test]
fn rejects_a_limb_wider_than_128_bits() {
    let mut limbs = signature(&vectors()["schnorr"][0]).to_limbs();
    limbs[1] += Base::from(1u128 << 127) * Base::from(2u64);

    assert!(Signature::from_limbs(&limbs).is_err());
}

#[test]
fn ome_signatures_verify_and_bind_the_message() {
    let mut rng = seeded(41);
    let key = SecretScalar::random(&mut rng);
    let public_key = Point::generator_mul_secret(&key).unwrap();
    let message = Base::from(99u64);

    let signature = schnorr::sign(&key, message, &mut rng).unwrap();

    assert!(schnorr::verify(&public_key, message, &signature));
    assert!(!schnorr::verify(
        &public_key,
        message + Base::from(1u64),
        &signature
    ));
}
