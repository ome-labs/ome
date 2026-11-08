//! Signed messages: the signature covers every byte of version, header, body and signer, and binds the sender.

mod support;

use ome_wire::{
    Error, KeeperIdentity, Message, SignedMessage, decode, encode, limits::MAX_PAYLOAD_BYTES,
    messages::proposals::ProposalPayload,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use support::{header, identity, samples, signed};

#[test]
fn every_flipped_byte_is_rejected() {
    let bytes = encode(&signed(samples().remove(0).1)).unwrap();

    for i in 0..bytes.len() {
        let mut tampered = bytes.clone();
        tampered[i] ^= 0x01;
        let accepted = decode(&tampered).is_ok_and(|message| message.verify().is_ok());
        assert!(!accepted, "byte {i}");
    }
}

#[test]
fn the_expected_signer_is_enforced() {
    let message = signed(samples().remove(3).1);

    assert_eq!(message.verify_from(&identity(1).public()), Ok(()));
    assert_eq!(
        message.verify_from(&identity(2).public()),
        Err(Error::UnexpectedSigner)
    );

    let mut impersonated = message;
    impersonated.signer = identity(2).public();
    assert_eq!(
        impersonated.verify_from(&identity(2).public()),
        Err(Error::InvalidSignature)
    );
}

#[test]
fn signing_refuses_unsupported_versions_and_oversized_messages() {
    let body = samples().remove(3).1;
    assert_eq!(
        SignedMessage::sign(&identity(1), 1, header(1), body).err(),
        Some(Error::UnsupportedVersion(1))
    );

    let oversized = Message::Payload(ProposalPayload {
        bytes: vec![0; MAX_PAYLOAD_BYTES + 1],
    });
    assert_eq!(
        SignedMessage::sign(&identity(1), 0, header(1), oversized).err(),
        Some(Error::OutOfBounds { field: "payload" })
    );
}

#[test]
fn an_identity_restores_from_its_seed_and_never_prints_it() {
    let original = KeeperIdentity::generate(&mut ChaCha20Rng::seed_from_u64(1));
    let restored = KeeperIdentity::from_seed(&original.seed());

    assert_eq!(restored.public(), original.public());
    let printed = format!("{original:?}");
    assert!(!printed.contains(&format!("{:?}", *original.seed())));
}
