//! Real nodes on loopback: frames cross QUIC and TCP+Noise, a keeper with no direct address is reached through a relay
//! circuit, and the gate refuses peers the receiver does not admit.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use std::time::Duration;

use ome_net::{Channel, Error, Libp2pTransport, Peer, Transport};
use support::{ceremony_frame, config, keeper, lobby_frame, next, relay, sync_frame};

const WAIT: Duration = Duration::from_secs(5);

fn peer_of(n: u8, node: &Libp2pTransport) -> Peer {
    support::peer(n, node.addresses())
}

async fn exchange_over(listen: &str) {
    let (alice, _) = Libp2pTransport::start(&keeper(1), config(&[listen], vec![]))
        .await
        .unwrap();
    let (bob, mut bob_inbound) = Libp2pTransport::start(&keeper(2), config(&[listen], vec![]))
        .await
        .unwrap();
    alice.set_admission(vec![peer_of(2, &bob)], false);
    bob.set_admission(vec![peer_of(1, &alice)], false);

    alice.send(peer_of(2, &bob), sync_frame(1)).await.unwrap();

    let inbound = next(&mut bob_inbound, WAIT).await.unwrap();
    assert_eq!(inbound.channel, Channel::Peer(keeper(1).public()));
    assert_eq!(inbound.bytes, sync_frame(1));
    let status = alice.peer_status(&[keeper(2).public()])[0];
    assert!(status.connected && !status.relayed);
    alice.stop();
    bob.stop();
}

#[tokio::test]
async fn frames_cross_tcp_with_noise() {
    exchange_over("/ip4/127.0.0.1/tcp/0").await;
}

#[tokio::test]
async fn frames_cross_quic() {
    exchange_over("/ip4/127.0.0.1/udp/0/quic-v1").await;
}

#[tokio::test]
async fn a_keeper_without_a_direct_address_is_reached_through_a_relay() {
    let (relay, _) = relay::spawn(9).await;
    let (alice, _) = Libp2pTransport::start(
        &keeper(1),
        config(&["/ip4/127.0.0.1/tcp/0"], vec![relay.clone()]),
    )
    .await
    .unwrap();
    let (carol, mut carol_inbound) = Libp2pTransport::start(&keeper(3), config(&[], vec![relay]))
        .await
        .unwrap();
    assert!(
        carol.addresses().iter().all(|a| a.contains("p2p-circuit")),
        "{:?}",
        carol.addresses()
    );
    alice.set_admission(vec![peer_of(3, &carol)], false);
    carol.set_admission(vec![peer_of(1, &alice)], false);

    alice
        .send(peer_of(3, &carol), ceremony_frame(1))
        .await
        .unwrap();

    let inbound = next(&mut carol_inbound, WAIT).await.unwrap();
    assert_eq!(inbound.channel, Channel::Peer(keeper(1).public()));
    assert!(alice.peer_status(&[keeper(3).public()])[0].relayed);
    alice.stop();
    carol.stop();
}

#[tokio::test]
async fn the_receiver_refuses_a_peer_it_does_not_admit() {
    let listen = ["/ip4/127.0.0.1/tcp/0"];
    let (alice, _) = Libp2pTransport::start(&keeper(1), config(&listen, vec![]))
        .await
        .unwrap();
    let (bob, mut bob_inbound) = Libp2pTransport::start(&keeper(2), config(&listen, vec![]))
        .await
        .unwrap();
    alice.set_admission(vec![peer_of(2, &bob)], false);

    assert!(
        alice
            .send(peer_of(2, &bob), ceremony_frame(1))
            .await
            .is_err()
    );
    assert_eq!(
        bob.send(peer_of(1, &alice), ceremony_frame(2)).await,
        Err(Error::NotAdmitted)
    );
    assert!(
        next(&mut bob_inbound, Duration::from_millis(300))
            .await
            .is_none()
    );

    // An open lobby lets the stranger in, on the lobby lane only.
    bob.set_admission(vec![], true);
    alice.send(peer_of(2, &bob), lobby_frame(1)).await.unwrap();
    assert_eq!(
        next(&mut bob_inbound, WAIT).await.unwrap().bytes,
        lobby_frame(1)
    );
    assert!(
        alice
            .send(peer_of(2, &bob), ceremony_frame(1))
            .await
            .is_err()
    );
    assert!(
        next(&mut bob_inbound, Duration::from_millis(300))
            .await
            .is_none()
    );
    alice.stop();
    bob.stop();
}
