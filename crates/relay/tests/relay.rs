//! The relay's circuits: a keeper with no direct address is reached through the real relay, and an allow list keeps
//! other keepers out.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use std::time::Duration;

use ome_net::{Channel, Libp2pTransport, Peer, Transport};
use support::{frame, identity, node, peer_id, relay};

fn peer(n: u8, transport: &Libp2pTransport) -> Peer {
    Peer {
        identity: identity(n).public(),
        transport: [n; 32],
        addresses: transport.addresses().to_vec(),
    }
}

#[tokio::test]
async fn a_keeper_without_a_direct_address_is_reached_through_the_relay() {
    let (mut relay, address) = relay(9, None).await;
    let (alice, _) = Libp2pTransport::start(
        &identity(1),
        node(&["/ip4/127.0.0.1/tcp/0"], vec![address.clone()]),
    )
    .await
    .unwrap();
    let (carol, mut inbound) = Libp2pTransport::start(&identity(3), node(&[], vec![address]))
        .await
        .unwrap();
    assert!(
        carol.addresses().iter().all(|a| a.contains("p2p-circuit")),
        "{:?}",
        carol.addresses()
    );
    alice.set_admission(vec![peer(3, &carol)], false);
    carol.set_admission(vec![peer(1, &alice)], false);

    alice.send(peer(3, &carol), frame(1)).await.unwrap();

    let received = tokio::time::timeout(Duration::from_secs(5), inbound.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.channel, Channel::Peer(identity(1).public()));
    assert_eq!(received.bytes, frame(1));
    assert!(alice.peer_status(&[identity(3).public()])[0].relayed);
    assert!(
        relay
            .metrics
            .circuits
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1
    );
    alice.stop();
    carol.stop();
    relay.stop();
}

#[tokio::test]
async fn a_private_relay_serves_only_its_allow_list() {
    let (mut relay, address) = relay(9, Some([peer_id(1)].into())).await;

    let (member, _) = Libp2pTransport::start(&identity(1), node(&[], vec![address.clone()]))
        .await
        .unwrap();
    let (stranger, _) = Libp2pTransport::start(&identity(2), node(&[], vec![address]))
        .await
        .unwrap();

    assert!(member.addresses().iter().any(|a| a.contains("p2p-circuit")));
    assert_eq!(stranger.addresses(), Vec::<String>::new());
    member.stop();
    stranger.stop();
    relay.stop();
}
