//! Real nodes on loopback: frames cross QUIC and TCP+Noise, and the gate refuses peers the receiver does not admit.
//! Relay circuits and relay mailboxes are tested against the real relay in `ome-relay`.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use std::time::Duration;

use ome_net::{Channel, Error, Libp2pTransport, Peer, Transport};
use support::{ceremony_frame, config, keeper, lobby_frame, next, sync_frame};

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
