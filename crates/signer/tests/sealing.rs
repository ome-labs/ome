//! Sealing: a key opens only with its secret, and a sealed file cannot be edited into something weaker or someone
//! else's.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_signer::{Error, SealedKey, SealingKey, Secret, Signer, SigningKey};
use serde_json::Value;
use support::seeded;

fn sealed_with_passphrase() -> (SigningKey, SealedKey) {
    let mut rng = seeded(1);
    let key = SigningKey::generate(&mut rng).unwrap();
    let sealed = SealedKey::seal(&key, &Secret::Passphrase("correct horse"), &mut rng).unwrap();
    (key, sealed)
}

/// `sealed` with one JSON field replaced.
fn edited(sealed: &SealedKey, pointer: &str, value: Value) -> String {
    let mut json: Value = serde_json::from_str(&sealed.to_json().unwrap()).unwrap();
    *json.pointer_mut(pointer).unwrap() = value;
    json.to_string()
}

#[test]
fn a_passphrase_sealed_key_opens_after_storage() {
    let (key, sealed) = sealed_with_passphrase();

    let stored = SealedKey::from_json(&sealed.to_json().unwrap()).unwrap();

    assert_eq!(stored.public_key().unwrap(), key.public_key());
    assert_eq!(
        stored
            .open(&Secret::Passphrase("correct horse"))
            .unwrap()
            .public_key(),
        key.public_key()
    );
    assert_eq!(
        stored.open(&Secret::Passphrase("wrong horse")).err(),
        Some(Error::WrongSecret)
    );
}

#[test]
fn the_derived_sealing_key_opens_without_the_passphrase() {
    let (key, sealed) = sealed_with_passphrase();

    let derived = sealed.sealing_key("correct horse").unwrap();

    assert_eq!(
        sealed.open(&Secret::Key(&derived)).unwrap().public_key(),
        key.public_key()
    );
    assert_eq!(
        sealed.sealing_key("wrong horse").err(),
        Some(Error::WrongSecret)
    );
}

#[test]
fn a_platform_sealed_key_needs_the_platform_key() {
    let mut rng = seeded(2);
    let key = SigningKey::generate(&mut rng).unwrap();
    let platform = SealingKey::from_bytes([7; 32]);

    let sealed = SealedKey::seal(&key, &Secret::Key(&platform), &mut rng).unwrap();

    assert_eq!(
        sealed.open(&Secret::Key(&platform)).unwrap().public_key(),
        key.public_key()
    );
    assert_eq!(
        sealed
            .open(&Secret::Key(&SealingKey::from_bytes([8; 32])))
            .err(),
        Some(Error::WrongSecret)
    );
    assert_eq!(
        sealed.open(&Secret::Passphrase("anything")).err(),
        Some(Error::WrongSecretKind {
            expected: "platform key"
        })
    );
}

#[test]
fn an_edited_file_does_not_open() {
    let (_, sealed) = sealed_with_passphrase();
    let other = SigningKey::generate(&mut seeded(3)).unwrap().public_key();
    let original: Value = serde_json::from_str(&sealed.to_json().unwrap()).unwrap();
    let mut flipped = original["ciphertext"].as_str().unwrap().to_owned();
    flipped.replace_range(0..1, if flipped.starts_with('0') { "1" } else { "0" });
    let x = format!(
        "0x{}",
        hex::encode(ome_crypto::curve::base_to_bytes(&other.x()))
    );
    let y = format!(
        "0x{}",
        hex::encode(ome_crypto::curve::base_to_bytes(&other.y()))
    );

    for text in [
        edited(&sealed, "/ciphertext", Value::String(flipped)),
        edited(&sealed, "/publicKey", serde_json::json!({ "x": x, "y": y })),
        edited(&sealed, "/kdf/passes", Value::from(4)),
    ] {
        let tampered = SealedKey::from_json(&text).unwrap();
        assert_eq!(
            tampered.open(&Secret::Passphrase("correct horse")).err(),
            Some(Error::WrongSecret)
        );
    }
}

#[test]
fn weakened_unknown_or_malformed_files_are_refused_before_opening() {
    let (_, sealed) = sealed_with_passphrase();
    let refused =
        |pointer, value: Value| SealedKey::from_json(&edited(&sealed, pointer, value)).err();

    assert_eq!(
        refused("/kdf/memoryKib", Value::from(1024)),
        Some(Error::KdfOutOfRange("memory"))
    );
    assert_eq!(
        refused("/kdf/passes", Value::from(1)),
        Some(Error::KdfOutOfRange("passes"))
    );
    assert_eq!(
        refused("/kdf/memoryKib", Value::from(u32::MAX)),
        Some(Error::KdfOutOfRange("memory"))
    );
    assert_eq!(
        refused("/v", Value::from("ome.signer.v9")),
        Some(Error::UnsupportedVersion("ome.signer.v9".into()))
    );
    assert_eq!(
        refused("/kdf/alg", Value::from("scrypt")),
        Some(Error::Malformed("json"))
    );
    assert_eq!(
        SealedKey::from_json(&"x".repeat(5000)).err(),
        Some(Error::Malformed("too large"))
    );
}

#[test]
fn a_reseal_keeps_the_key_and_takes_only_the_new_passphrase() {
    let (key, sealed) = sealed_with_passphrase();
    let mut rng = seeded(2);
    let old = Secret::Passphrase("correct horse");
    let new = Secret::Passphrase("battery staple");

    let resealed = sealed.reseal(&old, &new, &mut rng).unwrap();

    assert_eq!(resealed.public_key().unwrap(), key.public_key());
    assert_eq!(resealed.open(&new).unwrap().public_key(), key.public_key());
    assert!(matches!(resealed.open(&old), Err(Error::WrongSecret)));
    // A fresh salt: the sealing key changes with the passphrase.
    assert_ne!(
        resealed.sealing_key("battery staple").unwrap().to_bytes(),
        sealed.sealing_key("correct horse").unwrap().to_bytes()
    );
    // A wrong current passphrase changes nothing.
    assert!(matches!(
        sealed.reseal(&Secret::Passphrase("wrong"), &new, &mut rng),
        Err(Error::WrongSecret)
    ));
    // The sealing key opens it as the passphrase does.
    let by_key = sealed
        .reseal(
            &Secret::Key(&sealed.sealing_key("correct horse").unwrap()),
            &new,
            &mut rng,
        )
        .unwrap();
    assert_eq!(by_key.open(&new).unwrap().public_key(), key.public_key());
}
