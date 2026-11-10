//! A threshold change keeps every signer and the committee; keepers adopt the new threshold only from an executed
//! `set_threshold` that the chain's config confirms.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::Base;
use ome_protocol::{
    Error,
    membership::rethresholded_manifest,
    payload::{Call, CallKind, selector},
    vault::constants::{SET_THRESHOLD_SELECTOR, SET_THRESHOLD_SIGNATURE},
};
use ome_wire::types::field;
use support::fixture::{fixture, request};

fn set_threshold(vault: Base, threshold: u32) -> Call {
    Call {
        signature: Some(SET_THRESHOLD_SIGNATURE.to_owned()),
        to: field(&vault),
        selector: SET_THRESHOLD_SELECTOR,
        kind: CallKind::Private,
        is_static: false,
        hide_msg_sender: false,
        args: vec![field(&Base::from(threshold))],
    }
}

#[test]
fn the_selector_is_set_thresholds() {
    assert_eq!(
        selector(SET_THRESHOLD_SIGNATURE).unwrap(),
        SET_THRESHOLD_SELECTOR
    );
}

#[test]
fn a_keeper_adopts_a_threshold_only_from_a_landed_change() {
    let f = fixture();
    let landed = f.config.with_threshold(3).unwrap();
    let mut calls = request(5).calls;
    calls.push(set_threshold(f.vault.address(), 3));

    let next = rethresholded_manifest(&f.manifest, &landed, &calls).unwrap();
    assert_eq!(next.threshold, 3);
    assert_eq!(next.members, f.manifest.members);
    assert_eq!(next.epoch, f.manifest.epoch);

    // The chain still holds the old threshold: the change has not landed.
    assert_eq!(
        rethresholded_manifest(&f.manifest, &f.config, &calls),
        Err(Error::ManifestMismatch("threshold"))
    );
    // No call sets the threshold, or one sets another vault's.
    assert_eq!(
        rethresholded_manifest(&f.manifest, &landed, &request(5).calls),
        Err(Error::NotAThresholdChange)
    );
    assert_eq!(
        rethresholded_manifest(&f.manifest, &landed, &[set_threshold(Base::from(9u64), 3)]),
        Err(Error::NotAThresholdChange)
    );
}
