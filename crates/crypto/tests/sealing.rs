//! HPKE envelopes and mailboxes: a sealed payload opens only for its recipient, from its sender, in its context. A
//! pairing request opens only under its offer's secret.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{
    Error, SecretScalar,
    sealing::{
        envelope::{
            Context, TransportPublic, TransportSecret, open, open_scalar, seal, seal_scalar,
        },
        mailbox, pairing,
    },
};
use support::seeded;

fn context() -> Context {
    Context {
        session: [3u8; 32],
        epoch: 4,
        kind: 1,
        sender: 1,
        recipient: 2,
    }
}

fn keys(seed: u64) -> (TransportSecret, TransportPublic) {
    TransportSecret::generate(&mut seeded(seed))
}

#[test]
fn a_share_opens_for_its_recipient() {
    let mut rng = seeded(31);
    let (alice, alice_public) = keys(1);
    let (bob, bob_public) = keys(2);
    assert_eq!(alice.public().unwrap(), alice_public);
    let share = SecretScalar::random(&mut rng);

    let envelope = seal_scalar(&context(), &alice, &bob_public, &share, &mut rng).unwrap();

    assert_eq!(
        open_scalar(&context(), &bob, &alice_public, &envelope)
            .unwrap()
            .expose(),
        share.expose()
    );
}

#[test]
fn an_envelope_fails_for_another_recipient_sender_context_or_a_flipped_bit() {
    let mut rng = seeded(32);
    let (alice, alice_public) = keys(1);
    let (bob, bob_public) = keys(2);
    let (carol, carol_public) = keys(3);
    let envelope = seal(&context(), &alice, &bob_public, b"share", &mut rng).unwrap();

    assert_eq!(
        open(&context(), &carol, &alice_public, &envelope).err(),
        Some(Error::EnvelopeOpen)
    );
    assert_eq!(
        open(&context(), &bob, &carol_public, &envelope).err(),
        Some(Error::EnvelopeOpen)
    );
    for other in [
        Context {
            session: [4u8; 32],
            ..context()
        },
        Context {
            epoch: 5,
            ..context()
        },
        Context {
            kind: 2,
            ..context()
        },
        Context {
            sender: 3,
            ..context()
        },
        Context {
            recipient: 3,
            ..context()
        },
    ] {
        assert_eq!(
            open(&other, &bob, &alice_public, &envelope).err(),
            Some(Error::EnvelopeOpen)
        );
    }

    let mut tampered = envelope.clone();
    tampered.ciphertext[0] ^= 1;
    assert_eq!(
        open(&context(), &bob, &alice_public, &tampered).err(),
        Some(Error::EnvelopeOpen)
    );
}

#[test]
fn both_parties_derive_the_mailbox_and_it_separates_direction_and_day() {
    let (alice, alice_public) = keys(1);
    let (bob, bob_public) = keys(2);
    let (_, carol_public) = keys(3);

    let from_alice =
        mailbox::mailbox_id(&alice, &bob_public, &alice_public, &bob_public, 100).unwrap();
    let from_bob =
        mailbox::mailbox_id(&bob, &alice_public, &alice_public, &bob_public, 100).unwrap();

    assert_eq!(from_alice, from_bob);
    assert_ne!(
        mailbox::mailbox_id(&alice, &bob_public, &bob_public, &alice_public, 100).unwrap(),
        from_alice
    );
    assert_ne!(
        mailbox::mailbox_id(&alice, &bob_public, &alice_public, &bob_public, 101).unwrap(),
        from_alice
    );
    assert_ne!(
        mailbox::mailbox_id(&alice, &carol_public, &alice_public, &carol_public, 100).unwrap(),
        from_alice
    );
}

#[test]
fn a_mailbox_payload_opens_only_in_its_mailbox_from_its_sender() {
    let mut rng = seeded(33);
    let (alice, alice_public) = keys(1);
    let (bob, bob_public) = keys(2);
    let (carol, _) = keys(3);
    let id = mailbox::mailbox_id(&alice, &bob_public, &alice_public, &bob_public, 7).unwrap();

    let sealed = mailbox::seal(&id, 7, &alice, &bob_public, b"signed frame", &mut rng).unwrap();

    assert_eq!(
        mailbox::open(&id, 7, &bob, &alice_public, &sealed)
            .unwrap()
            .as_slice(),
        b"signed frame"
    );
    assert!(mailbox::open(&id, 8, &bob, &alice_public, &sealed).is_err());
    assert!(mailbox::open(&[0; 32], 7, &bob, &alice_public, &sealed).is_err());
    assert!(mailbox::open(&id, 7, &carol, &alice_public, &sealed).is_err());
    assert!(mailbox::open(&id, 7, &bob, &bob_public, &sealed).is_err());
    assert!(mailbox::open(&id, 7, &bob, &alice_public, &sealed[..10]).is_err());
}

#[test]
fn a_low_order_peer_key_is_refused() {
    let (alice, alice_public) = keys(1);
    let zero = TransportPublic([0; 32]);

    assert_eq!(
        mailbox::mailbox_id(&alice, &zero, &alice_public, &zero, 1).err(),
        Some(Error::EnvelopeKey)
    );
}

#[test]
fn a_pairing_request_opens_only_under_its_offer_secret() {
    let secret = [7u8; 32];
    let sealed = pairing::seal(&secret, b"pair me", &mut seeded(1)).unwrap();
    assert_eq!(
        pairing::open(&secret, &sealed).unwrap().as_slice(),
        b"pair me"
    );
    assert_eq!(pairing::open(&[8u8; 32], &sealed), Err(Error::EnvelopeOpen));
    let mut flipped = sealed.clone();
    *flipped.last_mut().unwrap() ^= 1;
    assert_eq!(pairing::open(&secret, &flipped), Err(Error::EnvelopeOpen));
    assert!(matches!(
        pairing::open(&secret, &sealed[..24]),
        Err(Error::InvalidLength { .. })
    ));
    assert_ne!(
        pairing::pairing_mailbox(&secret),
        pairing::pairing_mailbox(&[8u8; 32])
    );
}
