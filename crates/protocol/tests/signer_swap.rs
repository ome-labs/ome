//! A signer swap replaces one member's key and nothing else; keepers adopt the new key only from an executed swap that
//! the chain's config confirms.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{Point, SecretScalar};
use ome_protocol::{
    Error,
    membership::{Rotation, check_signer_swap, signer_swap_slot, swapped_manifest},
    vault::{Committee, SignerSet},
};
use ome_wire::types::WirePoint;
use support::fixture::{Fixture, fixture, request};

fn new_key(f: &mut Fixture) -> Point {
    Point::generator_mul_secret(&SecretScalar::random(&mut f.rng)).unwrap()
}

#[test]
fn a_swap_replaces_one_key_keeps_the_committee_and_bumps_the_version() {
    let mut f = fixture();
    let key = new_key(&mut f);
    let rotation = Rotation::signer_swap(&f.vault, 1, key).unwrap();

    let (slot, next) = check_signer_swap(&f.vault, &rotation).unwrap();

    assert_eq!(slot, 1);
    assert_eq!(next.version, f.config.version + 1);
    assert_eq!(next.committee_commitment, f.config.committee_commitment);
    assert_eq!(next.signer_set_commitment, rotation.signers.commitment());
}

#[test]
fn a_swap_that_changes_anything_else_is_refused() {
    let mut f = fixture();
    let key = new_key(&mut f);
    let swap = Rotation::signer_swap(&f.vault, 0, key).unwrap();

    let mut two_keys = swap.clone();
    let mut keys = two_keys.signers.keys().to_vec();
    keys[2] = new_key(&mut f);
    two_keys.signers = SignerSet::new(keys).unwrap();
    assert_eq!(
        signer_swap_slot(&f.vault, &two_keys),
        Err(Error::NotASignerSwap("it must replace exactly one key"))
    );

    let mut threshold = swap.clone();
    threshold.threshold = 3;
    assert!(matches!(
        signer_swap_slot(&f.vault, &threshold),
        Err(Error::NotASignerSwap(_))
    ));

    let mut committee = swap.clone();
    committee.committee = Committee::new(3, f.vault.committee().shares().to_vec()).unwrap();
    assert!(matches!(
        signer_swap_slot(&f.vault, &committee),
        Err(Error::NotASignerSwap(_))
    ));

    let unchanged = Rotation {
        signers: f.vault.signers().clone(),
        threshold: 2,
        committee: f.vault.committee().clone(),
    };
    assert!(matches!(
        signer_swap_slot(&f.vault, &unchanged),
        Err(Error::NotASignerSwap(_))
    ));
}

#[test]
fn a_keeper_adopts_the_new_key_only_from_a_landed_swap() {
    let mut f = fixture();
    let key = new_key(&mut f);
    let rotation = Rotation::signer_swap(&f.vault, 2, key).unwrap();
    let (_, landed) = check_signer_swap(&f.vault, &rotation).unwrap();
    let mut calls = request(5).calls;
    calls.push(rotation.to_call(&f.vault));

    let next = swapped_manifest(&f.manifest, &landed, &calls).unwrap();
    assert_eq!(next.members[2].signer_key, WirePoint::from(&key));
    assert_eq!(next.members[..2], f.manifest.members[..2]);
    assert_eq!(next.epoch, f.manifest.epoch);

    // The chain still holds the old signer set: the swap has not landed.
    assert_eq!(
        swapped_manifest(&f.manifest, &f.config, &calls),
        Err(Error::SignerSetMismatch)
    );
    // No call swaps a signer.
    assert!(matches!(
        swapped_manifest(&f.manifest, &landed, &request(5).calls),
        Err(Error::NotASignerSwap(_))
    ));
}
