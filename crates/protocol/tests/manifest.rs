//! A manifest is acted on only if it is the vault's actual committee, and each mismatch is named.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_protocol::{
    Error,
    membership::{Vault, policy},
    vault::Config,
};
use support::fixture::{config_of, fixture, manifest, run_dkg};

#[test]
fn a_manifest_matching_the_chain_becomes_a_vault() {
    let f = fixture();

    assert_eq!(f.vault.signers().count(), 3);
    assert_eq!(f.vault.config(), &f.config);
    assert_eq!(f.vault.group_key(), f.shares[0].group_public_key());
}

#[test]
fn each_disagreement_with_the_chain_is_named() {
    let f = fixture();
    let check = |change: fn(&mut Config)| {
        let mut config = f.config;
        change(&mut config);
        Vault::from_manifest(&f.manifest, &config)
    };

    assert_eq!(
        check(|c| c.threshold = 3),
        Err(Error::ManifestMismatch("threshold"))
    );
    assert_eq!(check(|c| c.signer_count = 2), Err(Error::SignerSetMismatch));
    assert_eq!(
        check(|c| c.signer_set_commitment += ome_crypto::Base::from(1u64)),
        Err(Error::SignerSetMismatch)
    );
    assert_eq!(
        check(|c| c.committee_commitment += ome_crypto::Base::from(1u64)),
        Err(Error::CommitteeMismatch)
    );

    let mut future = f.manifest.clone();
    future.epoch = 1;
    assert_eq!(
        Vault::from_manifest(&future, &f.config),
        Err(Error::ManifestMismatch("epoch"))
    );
}

#[test]
fn shares_of_another_key_are_refused_even_when_the_chain_agrees() {
    let mut f = fixture();
    let other = run_dkg(2, 3, &mut f.rng);
    // The committee commitment is computed from the other key's shares, but the manifest keeps the vault's key.
    let mut forged = manifest(&f.signing_keys, &other, 2);
    forged.group_key = f.manifest.group_key;
    forged.vault = f.manifest.vault;

    assert_eq!(
        Vault::from_manifest(&forged, &config_of(&forged)),
        Err(Error::GroupKeyMismatch)
    );
}

#[test]
fn risky_thresholds_are_reported() {
    use policy::Warning::{NoSpareSigner, NoSpareViewer, SingleSigner, TwoCanRead};

    assert_eq!(policy::warnings(2, 2, 3), vec![TwoCanRead]);
    assert_eq!(policy::warnings(1, 3, 3), vec![SingleSigner, NoSpareViewer]);
    assert_eq!(policy::warnings(3, 2, 3), vec![NoSpareSigner, TwoCanRead]);
    assert_eq!(policy::warnings(3, 3, 5), vec![]);
}
