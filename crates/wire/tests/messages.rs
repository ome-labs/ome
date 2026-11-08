//! Message contents: conversions to `ome-crypto` types keep their meaning and refuse invalid values, manifests are
//! checked structurally, and a proposal binds its payload.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{
    Base, Error as CryptoError, Point, SecretScalar,
    aztec::AddressContext,
    schnorr,
    sealing::envelope::{Context, TransportSecret, open_scalar, seal_scalar},
    threshold::{
        dkg::{self, Parameters, Round1Package},
        ecdh::{AddressSecretShare, EcdhContext, verify_partial},
        reshare::DealerPackage,
        shamir::Polynomial,
    },
};
use ome_wire::{
    Error,
    messages::{
        ceremony::{Ceremony, DkgRound1, Justification, Manifest, ReshareDeal, SealedShare},
        ecdh::EcdhResponse,
        proposals::{Approval, ProposalPayload},
    },
    types::{WirePoint, field, parse_scalar, scalar},
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use support::{identity, manifest, payload, proposal};

fn rng(seed: u64) -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(seed)
}

#[test]
fn ceremony_broadcasts_survive_the_wire() {
    let mut rng = rng(1);
    let parameters = Parameters {
        session: [1; 32],
        threshold: 2,
        participants: vec![1, 2, 3],
    };
    let (_, package) = dkg::round1(&parameters, 2, &mut rng).unwrap();
    assert_eq!(
        Round1Package::try_from(&DkgRound1::from(&package)).unwrap(),
        package
    );

    let polynomial = Polynomial::random(SecretScalar::random(&mut rng), 2, &mut rng).unwrap();
    let deal = DealerPackage {
        dealer: 3,
        commitments: polynomial.commitments().unwrap(),
    };
    assert_eq!(
        DealerPackage::try_from(&ReshareDeal::from(&deal)).unwrap(),
        deal
    );
}

#[test]
fn ecdh_partials_survive_the_wire_and_still_verify() {
    let mut rng = rng(2);
    let ivpk = Point::generator_mul_secret(&SecretScalar::random(&mut rng)).unwrap();
    let address = AddressContext::new(Base::from(7u64), &ivpk).unwrap();
    let share = AddressSecretShare::new(1, &SecretScalar::random(&mut rng), &address).unwrap();
    let context = EcdhContext {
        vault: address.address(),
        epoch: 0,
    };
    let input = Point::generator_mul_secret(&SecretScalar::random(&mut rng)).unwrap();
    let partial = share.partial(&input, &context, &mut rng).unwrap();

    let received = EcdhResponse::new([0; 16], 1, &[partial])
        .partials()
        .unwrap();

    assert_eq!(received, vec![partial]);
    assert_eq!(
        verify_partial(&received[0], &input, share.public(), &context),
        Ok(())
    );
}

#[test]
fn a_sealed_share_opens_after_the_wire() {
    let mut rng = rng(3);
    let (dealer, dealer_public) = TransportSecret::generate(&mut rng);
    let (recipient, recipient_public) = TransportSecret::generate(&mut rng);
    let context = Context {
        session: [2; 32],
        epoch: 0,
        kind: 1,
        sender: 1,
        recipient: 2,
    };
    let share = SecretScalar::random(&mut rng);
    let envelope = seal_scalar(&context, &dealer, &recipient_public, &share, &mut rng).unwrap();

    let sealed = SealedShare::new(1, 2, envelope);

    let opened = open_scalar(&context, &recipient, &dealer_public, &sealed.envelope()).unwrap();
    assert_eq!(opened.expose(), share.expose());
}

#[test]
fn invalid_points_and_scalars_are_refused() {
    let off_curve = WirePoint {
        x: field(&Base::from(1u64)),
        y: field(&Base::from(1u64)),
    };

    assert_eq!(
        Point::try_from(&off_curve),
        Err(Error::Crypto(CryptoError::PointNotOnCurve))
    );
    assert_eq!(
        parse_scalar(&[0xff; 32]),
        Err(Error::Crypto(CryptoError::NonCanonicalEncoding))
    );
}

#[test]
fn a_justification_settles_the_complaint() {
    let mut rng = rng(4);
    let polynomial = Polynomial::random(SecretScalar::random(&mut rng), 2, &mut rng).unwrap();
    let commitments = polynomial.commitments().unwrap();
    let justification = |share: &SecretScalar| Justification {
        ceremony: Ceremony::Dkg,
        complainant: 2,
        share: scalar(share.expose()),
    };

    assert_eq!(
        justification(&polynomial.evaluate(2).unwrap()).holds(&commitments),
        Ok(true)
    );
    assert_eq!(
        justification(&polynomial.evaluate(3).unwrap()).holds(&commitments),
        Ok(false)
    );
}

/// A change that breaks one structural rule of a manifest.
type Fault = fn(&mut Manifest);

#[test]
fn a_manifest_is_refused_for_each_structural_fault() {
    let cases: [(Fault, Error); 7] = [
        (|m| m.protocol_version = 9, Error::UnsupportedVersion(9)),
        (
            |m| m.members.clear(),
            Error::OutOfBounds { field: "members" },
        ),
        (
            |m| m.threshold = 0,
            Error::OutOfBounds { field: "threshold" },
        ),
        (
            |m| m.threshold = 4,
            Error::OutOfBounds { field: "threshold" },
        ),
        (
            |m| m.viewing_threshold = 4,
            Error::OutOfBounds {
                field: "viewing_threshold",
            },
        ),
        (
            |m| m.members.swap(0, 1),
            Error::OutOfBounds {
                field: "members.index",
            },
        ),
        (
            |m| m.members[2].identity = identity(1).public(),
            Error::OutOfBounds {
                field: "members.identity",
            },
        ),
    ];

    assert_eq!(manifest().validate(), Ok(()));
    for (fault, expected) in cases {
        let mut faulty = manifest();
        fault(&mut faulty);
        assert_eq!(faulty.validate(), Err(expected));
    }
}

#[test]
fn a_manifest_vault_must_follow_from_its_keys() {
    let mut rng = rng(5);
    let ivpk = Point::generator_mul_secret(&SecretScalar::random(&mut rng)).unwrap();
    let preaddress = Base::from(42u64);
    let address = AddressContext::new(preaddress, &ivpk).unwrap();
    let mut consistent = manifest();
    consistent.preaddress = field(&preaddress);
    consistent.group_key = WirePoint::from(&ivpk);
    consistent.vault = field(&address.address());

    assert_eq!(
        consistent.address_context().unwrap().address(),
        address.address()
    );

    consistent.vault = field(&Base::from(1u64));
    assert_eq!(
        consistent.address_context().err(),
        Some(Error::OutOfBounds { field: "vault" })
    );
}

#[test]
fn a_proposal_binds_its_payload() {
    let other = ProposalPayload {
        bytes: vec![9, 9, 9, 9],
    };

    assert!(proposal().payload.matches(&payload()));
    assert!(!proposal().payload.matches(&other));

    let mut swapped = proposal();
    swapped.payload = other.reference().unwrap();
    assert_ne!(swapped.id().unwrap(), proposal().id().unwrap());
}

#[test]
fn an_approval_carries_a_signature_that_still_verifies() {
    let mut rng = rng(6);
    let key = SecretScalar::random(&mut rng);
    let message = Base::from(77u64);
    let signature = schnorr::sign(&key, message, &mut rng).unwrap();

    let approval = Approval::new(proposal().id().unwrap(), 0, &signature);

    let received = schnorr::Signature::try_from(&approval.signature).unwrap();
    assert!(schnorr::verify(
        &Point::generator_mul_secret(&key).unwrap(),
        message,
        &received
    ));
}
