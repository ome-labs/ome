//! The in-process network delivers like libp2p: authenticated channels, the same admission, partitions on demand.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use std::time::Duration;

use ome_net::{Channel, Error, MemoryNetwork, Transport};
use support::{ceremony_frame, keeper, lobby_frame, next, peer};

#[tokio::test]
async fn members_exchange_frames_on_authenticated_channels() {
    let network = MemoryNetwork::new();
    let (alice, _) = network.endpoint(keeper(1).public());
    let (bob, mut bob_inbound) = network.endpoint(keeper(2).public());
    alice.set_admission(vec![peer(2, &[])], false);
    bob.set_admission(vec![peer(1, &[])], false);

    alice.send(peer(2, &[]), ceremony_frame(1)).await.unwrap();

    let inbound = next(&mut bob_inbound, Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(inbound.channel, Channel::Peer(keeper(1).public()));
    assert_eq!(inbound.bytes, ceremony_frame(1));
    let status = alice.peer_status(&[keeper(2).public()])[0];
    assert!(status.connected && !status.relayed);
}

#[tokio::test]
async fn admission_is_enforced_on_both_ends() {
    let network = MemoryNetwork::new();
    let (alice, _) = network.endpoint(keeper(1).public());
    let (bob, _bob_inbound) = network.endpoint(keeper(2).public());
    alice.set_admission(vec![peer(2, &[])], false);

    assert_eq!(
        bob.send(peer(1, &[]), ceremony_frame(2)).await,
        Err(Error::NotAdmitted)
    );
    assert_eq!(
        alice.send(peer(2, &[]), ceremony_frame(1)).await,
        Err(Error::NotAdmitted)
    );
    assert!(!alice.peer_status(&[keeper(2).public()])[0].connected);

    bob.set_admission(vec![], true);
    assert_eq!(alice.send(peer(2, &[]), lobby_frame(1)).await, Ok(()));
    assert_eq!(
        alice.send(peer(2, &[]), ceremony_frame(1)).await,
        Err(Error::NotAdmitted)
    );
}

#[tokio::test]
async fn a_partition_or_a_departed_peer_is_unreachable() {
    let network = MemoryNetwork::new();
    let (alice, _) = network.endpoint(keeper(1).public());
    let (bob, _bob_inbound) = network.endpoint(keeper(2).public());
    alice.set_admission(vec![peer(2, &[])], false);
    bob.set_admission(vec![peer(1, &[])], false);

    network.set_filter(|_, _, _| false);
    assert_eq!(
        alice.send(peer(2, &[]), ceremony_frame(1)).await,
        Err(Error::Unreachable)
    );
    assert!(!alice.peer_status(&[keeper(2).public()])[0].connected);

    network.clear_filter();
    network.leave(&keeper(2).public());
    assert_eq!(
        alice.send(peer(2, &[]), ceremony_frame(1)).await,
        Err(Error::Unreachable)
    );
}
