//! Custody across rotations: a keeper swaps or destroys its share only for a committee its own reshare produced and
//! the whole config confirms; anything else keeps the share and raises an alarm. Pending shares of rotations that
//! never land expire. Restarts keep vaults and abort interrupted ceremonies.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_api::{
    Hex32,
    keeper::{CeremonyStatus, KeeperEvent},
};
use ome_crypto::Base;
use ome_keeper::{Error, KeeperConfig, Observation, ReshareRequest, ceremony::dkg::DkgParams};
use ome_wire::{messages::ecdh::EcdhPurpose, types::parse_field};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use support::{
    Vault, ceremony_reaches, config_of, ecdh, fast_config, new_member, point, run_reshare, vault,
    wait_for,
};

/// Keepers 1 and 3 deal; keeper 3 leaves and `newcomer` joins: the new committee is 1, 2, newcomer.
fn rotation(v: &Vault, newcomer: &support::TestKeeper, session: u8) -> ReshareRequest {
    let mut rng = ChaCha20Rng::seed_from_u64(u64::from(session));
    ReshareRequest {
        session: [session; 32],
        vault: v.address,
        current: None,
        dealers: vec![1, 3],
        new_threshold: 2,
        new_spending_threshold: 2,
        new_members: [&v.keepers[0], &v.keepers[1], newcomer]
            .iter()
            .map(|k| new_member(k, &mut rng))
            .collect(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_verified_rotation_activates_the_new_share_and_removes_the_leaver() {
    let v = vault(fast_config()).await;
    let points = [point(4)];
    let before = ecdh(&v.keepers[0].keeper, v.address, 0, &points).await;
    let newcomer = v.harness.keeper(4);
    let request = rotation(&v, &newcomer, 2);
    let participants = [&v.keepers[0], &v.keepers[1], &v.keepers[2]];

    let result = run_reshare(&participants, &[&newcomer], &request, &v.manifest).await;

    // Until the chain names the new committee, the old one keeps serving.
    assert_eq!(
        ecdh(&v.keepers[2].keeper, v.address, 0, &points).await,
        before
    );
    let mut next = result.manifest.unwrap();
    next.epoch = 1;
    let config = config_of(&next, 1);
    assert_eq!(
        parse_field(&result.committee_commitment).unwrap(),
        config.committee_commitment
    );
    for k in [&v.keepers[0], &v.keepers[1], &newcomer] {
        assert_eq!(
            k.keeper.observe_config(v.address, config).await.unwrap(),
            Observation::Activated
        );
    }
    assert_eq!(
        v.keepers[2]
            .keeper
            .observe_config(v.address, config)
            .await
            .unwrap(),
        Observation::Removed
    );

    // Same secret from the new committee; the old shares are gone, and the leaver can no longer take part.
    assert_eq!(ecdh(&newcomer.keeper, v.address, 1, &points).await, before);
    assert_eq!(
        ecdh(&v.keepers[0].keeper, v.address, 1, &points).await,
        before
    );
    let vault = hex::encode(v.address);
    assert_eq!(
        v.keepers[0].secrets("share-"),
        vec![format!("share-{vault}-1")]
    );
    assert_eq!(v.keepers[2].secrets("share-"), Vec::<String>::new());
    let refused = v.keepers[2]
        .keeper
        .ecdh_batch(v.address, 1, EcdhPurpose::NoteDecryption, points.to_vec())
        .await;
    assert!(matches!(refused, Err(Error::Removed)));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unverifiable_config_keeps_the_share_and_raises_an_alarm() {
    let v = vault(fast_config()).await;
    let mut events = v.keepers[0].keeper.subscribe();
    let points = [point(5)];
    let before = ecdh(&v.keepers[0].keeper, v.address, 0, &points).await;
    // A newer config naming a committee no reshare of ours produced, as a buggy or malicious rotation would.
    let garbage = ome_protocol::vault::Config {
        version: 1,
        committee_commitment: Base::from(12_345u64),
        ..v.config
    };

    let observed = v.keepers[0]
        .keeper
        .observe_config(v.address, garbage)
        .await
        .unwrap();

    assert_eq!(observed, Observation::Unknown);
    let version = wait_for(&mut events, |e| match e {
        KeeperEvent::ConfigMismatch { version, .. } => Some(*version),
        _ => None,
    })
    .await;
    assert_eq!(version, 1);
    assert!(v.keepers[0].keeper.vault(v.address).await.unwrap().serving);
    assert_eq!(
        ecdh(&v.keepers[0].keeper, v.address, 0, &points).await,
        before
    );
    assert_eq!(
        v.keepers[0]
            .keeper
            .observe_config(v.address, v.config)
            .await
            .unwrap(),
        Observation::Current
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reshare_committee_with_a_mismatching_config_is_not_activated() {
    let v = vault(fast_config()).await;
    let newcomer = v.harness.keeper(4);
    let participants = [&v.keepers[0], &v.keepers[1], &v.keepers[2]];
    let result = run_reshare(
        &participants,
        &[&newcomer],
        &rotation(&v, &newcomer, 3),
        &v.manifest,
    )
    .await;
    let mut next = result.manifest.unwrap();
    next.epoch = 1;
    // The committee is ours, but the config's signer set is not the one the reshare was for.
    let tampered = ome_protocol::vault::Config {
        signer_set_commitment: Base::from(1u64),
        ..config_of(&next, 1)
    };

    let observed = v.keepers[0]
        .keeper
        .observe_config(v.address, tampered)
        .await
        .unwrap();

    assert_eq!(observed, Observation::Unknown);
    let vault = hex::encode(v.address);
    assert_eq!(
        v.keepers[0].secrets("share-"),
        vec![format!("share-{vault}-0")]
    );
    assert_eq!(v.keepers[0].secrets("pending-").len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pending_share_whose_rotation_never_lands_expires() {
    let v = vault(KeeperConfig {
        pending_share_ttl_secs: 600,
        ..fast_config()
    })
    .await;
    let newcomer = v.harness.keeper(4);
    let participants = [&v.keepers[0], &v.keepers[1], &v.keepers[2]];
    let request = rotation(&v, &newcomer, 4);
    run_reshare(&participants, &[&newcomer], &request, &v.manifest).await;
    assert_eq!(v.keepers[0].secrets("pending-").len(), 1);
    let mut events = v.keepers[0].keeper.subscribe();

    v.harness.clock.advance(601);

    let session = wait_for(&mut events, |e| match e {
        KeeperEvent::PendingExpired { session, .. } => Some(*session),
        _ => None,
    })
    .await;
    assert_eq!(session, Hex32(request.session));
    assert_eq!(v.keepers[0].secrets("pending-"), Vec::<String>::new());
    assert_eq!(
        v.keepers[0]
            .keeper
            .ceremony(request.session)
            .unwrap()
            .status,
        "expired"
    );
    assert!(v.keepers[0].keeper.vault(v.address).await.unwrap().serving);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_keeps_the_vault_and_aborts_an_interrupted_ceremony() {
    let v = vault(fast_config()).await;
    let points = [point(6)];
    let before = ecdh(&v.keepers[1].keeper, v.address, 0, &points).await;
    let params = DkgParams {
        session: [5; 32],
        threshold: 2,
        members: v
            .keepers
            .iter()
            .map(support::TestKeeper::participant)
            .collect(),
    };
    v.keepers[0].keeper.start_dkg(params).await.unwrap();

    v.harness.net.leave(&v.keepers[0].keeper.identity());
    let restarted = v
        .harness
        .start(v.keepers[0].dir.path(), v.keepers[0].master);

    let interrupted = restarted.ceremony([5; 32]).unwrap();
    assert_eq!(interrupted.status, "aborted");
    assert!(interrupted.failure.unwrap().reason.contains("restarted"));
    assert!(restarted.vault(v.address).await.unwrap().serving);
    assert_eq!(ecdh(&restarted, v.address, 0, &points).await, before);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_silent_member_times_the_ceremony_out_and_is_blamed() {
    let harness = support::Harness::new(fast_config());
    let keepers: Vec<_> = (1..=3).map(|i| harness.keeper(i)).collect();
    let params = DkgParams {
        session: [3; 32],
        threshold: 2,
        members: keepers
            .iter()
            .map(support::TestKeeper::participant)
            .collect(),
    };
    let mut events = keepers[0].keeper.subscribe();
    for k in &keepers[..2] {
        k.keeper.start_dkg(params.clone()).await.unwrap();
    }

    harness.clock.advance(31);

    ceremony_reaches(&mut events, [3; 32], CeremonyStatus::Aborted).await;
    let failure = keepers[0]
        .keeper
        .ceremony([3; 32])
        .unwrap()
        .failure
        .unwrap();
    assert_eq!(
        failure.blamed,
        vec![hex::encode(keepers[2].keeper.identity().0)]
    );
}
