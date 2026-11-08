//! Threshold ECDH between keepers: every member gathers the same verified secret, requests outside the committee or
//! epoch are refused, a lying member is reported and routed around, and peers enforce a budget. Every answer is
//! audited.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{SecretScalar, threshold::ecdh::AddressSecretShare};
use ome_keeper::{Clock, Error, KeeperConfig, committee::Committee, events::Event};
use ome_net::Transport;
use ome_wire::{
    Header, Message, SignedMessage, decode, encode,
    messages::ecdh::{EcdhPurpose, EcdhResponse},
    types::{WirePoint, parse_points},
    version::PROTOCOL_VERSION,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use support::{ecdh, fast_config, point, vault, wait_for};

#[tokio::test(flavor = "multi_thread")]
async fn every_member_gathers_the_same_verified_secret() {
    let v = vault(fast_config()).await;
    let points = [point(1), point(2)];

    let first = ecdh(&v.keepers[0].keeper, v.address, 0, &points).await;

    for k in &v.keepers[1..] {
        assert_eq!(ecdh(&k.keeper, v.address, 0, &points).await, first);
    }
    let audit = v.keepers[1].keeper.audit_log(v.address, 10).unwrap();
    assert!(
        audit
            .iter()
            .any(|e| e.requester == "local" && e.outcome == "served")
    );
    assert!(
        audit
            .iter()
            .any(|e| e.requester != "local" && e.outcome == "served")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn other_epochs_and_unknown_vaults_are_refused() {
    let v = vault(fast_config()).await;
    let batch = |vault, epoch| {
        v.keepers[0]
            .keeper
            .ecdh_batch(vault, epoch, EcdhPurpose::NoteDecryption, vec![point(1)])
    };

    assert!(matches!(
        batch(v.address, 1).await,
        Err(Error::WrongEpoch {
            expected: 0,
            actual: 1
        })
    ));
    assert!(matches!(batch([9; 32], 0).await, Err(Error::UnknownVault)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lying_member_is_reported_and_routed_around() {
    let v = vault(fast_config()).await;
    let mut events = v.keepers[0].keeper.subscribe();
    // Member 2's endpoint is taken over by an impostor answering with partials from a wrong share.
    let keys = v.keepers[1].keys();
    let (impostor, mut inbox) = v.harness.net.endpoint(keys.identity.public());
    impostor.set_admission(vec![v.keepers[0].peer()], false);
    let committee = Committee::new(v.manifest.clone()).unwrap();
    let target = v.keepers[0].peer();
    let clock = v.harness.clock.clone();
    tokio::spawn(async move {
        let mut rng = ChaCha20Rng::seed_from_u64(3);
        while let Some(frame) = inbox.recv().await {
            let message = decode(&frame.bytes).unwrap();
            let Message::EcdhRequest(request) = message.body else {
                continue;
            };
            let wrong =
                AddressSecretShare::new(2, &SecretScalar::random(&mut rng), &committee.context)
                    .unwrap();
            let partials = wrong
                .partials(
                    &parse_points(&request.points).unwrap(),
                    &committee.ecdh_context(),
                    &mut rng,
                )
                .unwrap();
            let header = Header {
                scope: message.header.scope,
                epoch: 0,
                id: [7; 16],
                created_at: clock.now(),
            };
            let body = Message::EcdhResponse(EcdhResponse::new(message.header.id, 2, &partials));
            let frame = encode(
                &SignedMessage::sign(&keys.identity, PROTOCOL_VERSION, header, body).unwrap(),
            )
            .unwrap();
            impostor.send(target.clone(), frame).await.unwrap();
        }
    });
    let silent = v.keepers[2].keeper.identity();
    v.harness
        .net
        .set_filter(move |from, to, _| from != &silent && to != &silent);

    let result = v.keepers[0]
        .keeper
        .ecdh_batch(v.address, 0, EcdhPurpose::NoteDecryption, vec![point(3)])
        .await;

    assert!(matches!(
        result,
        Err(Error::Unavailable {
            needed: 2,
            received: 1
        })
    ));
    let reported = wait_for(&mut events, |e| match e {
        Event::InvalidPartials { member, .. } => Some(*member),
        _ => None,
    })
    .await;
    assert_eq!(reported, 2);

    v.harness.net.clear_filter();
    let gathered = v.keepers[0]
        .keeper
        .ecdh_batch(v.address, 0, EcdhPurpose::NoteDecryption, vec![point(3)])
        .await
        .unwrap();
    assert_eq!(
        gathered.iter().map(|m| m.member).collect::<Vec<_>>(),
        vec![1, 3]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn peers_enforce_the_ecdh_budget() {
    let v = vault(KeeperConfig {
        ecdh_points_per_minute: 4,
        ..fast_config()
    })
    .await;
    let points: Vec<WirePoint> = (10..18).map(point).collect();

    let result = v.keepers[0]
        .keeper
        .ecdh_batch(v.address, 0, EcdhPurpose::Handshake, points)
        .await;

    assert!(matches!(
        result,
        Err(Error::Unavailable {
            needed: 2,
            received: 1
        })
    ));
    let audit = v.keepers[1].keeper.audit_log(v.address, 10).unwrap();
    assert!(
        audit
            .iter()
            .any(|e| e.outcome.contains("rate limit") && e.purpose == "handshake")
    );
    ecdh(&v.keepers[0].keeper, v.address, 0, &[point(20)]).await;
}
