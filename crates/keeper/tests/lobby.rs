//! Founding lobbies: guests join with an invite, see the roster, and receive the plan with their sealed founding
//! secrets and the negotiated protocol version; a join with the wrong secret is refused; guests leave and hosts close.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_api::keeper::KeeperEvent;
use ome_keeper::{Keeper, LobbyInfo, LobbyOpen, LobbyRole, LobbyStatus};
use ome_wire::{
    messages::lobby::{Invite, JoinSecret},
    version::PROTOCOL_VERSION,
};
use support::{Harness, TestKeeper, fast_config, point, wait_for};
use tokio::sync::broadcast;

const SECRETS: &[u8] = b"founding secrets: deployment salt, contract class, shared settings";

fn open_request(name: &str) -> LobbyOpen {
    LobbyOpen {
        name: "Treasury".into(),
        threshold: 2,
        viewing_threshold: 2,
        members: 3,
        network: "local-network".into(),
        member_name: name.into(),
        member_account: [0x01; 32],
        signer_key: point(100),
    }
}

async fn info(keeper: &Keeper, lobby: [u8; 16]) -> LobbyInfo {
    keeper.lobby(lobby, true).await.unwrap()
}

/// Waits for a lobby event with `members` on the roster and `status`.
async fn lobby_event(
    events: &mut broadcast::Receiver<KeeperEvent>,
    status: LobbyStatus,
    members: u32,
) {
    wait_for(events, |e| match e {
        KeeperEvent::Lobby {
            status: s,
            members: m,
            ..
        } if *s == status && *m == members => Some(()),
        _ => None,
    })
    .await;
}

/// A host and two guests in an open lobby.
async fn lobby_of_three(harness: &Harness) -> ([TestKeeper; 3], [u8; 16], String) {
    let keepers = [harness.keeper(1), harness.keeper(2), harness.keeper(3)];
    let mut guests: Vec<_> = keepers[1..].iter().map(|k| k.keeper.subscribe()).collect();
    let (lobby, invite) = keepers[0]
        .keeper
        .open_lobby(open_request("Ada"))
        .await
        .unwrap();
    for (k, (name, seed)) in keepers[1..].iter().zip([("Grace", 101), ("Alan", 102)]) {
        assert_eq!(
            k.keeper
                .join_lobby(&invite, name.into(), [0x02; 32], point(seed))
                .await
                .unwrap(),
            lobby
        );
    }
    for events in &mut guests {
        lobby_event(events, LobbyStatus::Open, 3).await;
    }
    (keepers, lobby, invite)
}

#[tokio::test(flavor = "multi_thread")]
async fn guests_see_the_roster_and_receive_the_same_plan_and_secrets() {
    let harness = Harness::new(fast_config());
    let (keepers, lobby, invite) = lobby_of_three(&harness).await;
    let mut guests: Vec<_> = keepers[1..].iter().map(|k| k.keeper.subscribe()).collect();
    let host = info(&keepers[0].keeper, lobby).await;
    assert_eq!(
        (host.role, host.status),
        (LobbyRole::Host, LobbyStatus::Open)
    );
    assert_eq!(host.invite.as_deref(), Some(invite.as_str()));
    let guest = info(&keepers[1].keeper, lobby).await;
    assert_eq!(guest.role, LobbyRole::Guest);
    assert!(guest.invite.is_none() && guest.plan.is_none());
    assert_eq!(guest.members, host.members);

    let started = keepers[0]
        .keeper
        .start_lobby(lobby, 2, 2, [7; 32], SECRETS)
        .await
        .unwrap();

    let plan = started.plan.unwrap();
    assert_eq!(plan.secrets.as_deref().map(Vec::as_slice), Some(SECRETS));
    for (events, k) in guests.iter_mut().zip(&keepers[1..]) {
        lobby_event(events, LobbyStatus::Started, 3).await;
        let received = info(&k.keeper, lobby).await.plan.unwrap();
        assert_eq!((received.session, received.salt), (plan.session, [7; 32]));
        assert_eq!(
            received.secrets.as_deref().map(Vec::as_slice),
            Some(SECRETS)
        );
        assert_eq!(received.protocol_version, PROTOCOL_VERSION);
    }
    let listed = keepers[1].keeper.lobbies().await.unwrap();
    assert!(listed[0].plan.as_ref().is_some_and(|p| p.secrets.is_none()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_join_with_the_wrong_invite_secret_is_refused() {
    let harness = Harness::new(fast_config());
    let [host, honest, intruder] = [harness.keeper(1), harness.keeper(2), harness.keeper(3)];
    let mut host_events = host.keeper.subscribe();
    let (lobby, invite) = host.keeper.open_lobby(open_request("Ada")).await.unwrap();
    let mut forged = Invite::decode(&invite).unwrap();
    forged.secret = JoinSecret([0x55; 32]);

    intruder
        .keeper
        .join_lobby(
            &forged.encode().unwrap(),
            "Mallory".into(),
            [0x02; 32],
            point(103),
        )
        .await
        .unwrap();
    honest
        .keeper
        .join_lobby(&invite, "Grace".into(), [0x02; 32], point(101))
        .await
        .unwrap();

    lobby_event(&mut host_events, LobbyStatus::Open, 2).await;
    let names: Vec<String> = info(&host.keeper, lobby)
        .await
        .members
        .iter()
        .map(|m| m.name.clone())
        .collect();
    assert_eq!(names, ["Ada", "Grace"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guest_leaves_and_the_host_closes() {
    let harness = Harness::new(fast_config());
    let (keepers, lobby, _) = lobby_of_three(&harness).await;
    let mut host = keepers[0].keeper.subscribe();
    let mut remaining = keepers[1].keeper.subscribe();

    keepers[2]
        .keeper
        .close_lobby(lobby, LobbyRole::Guest)
        .await
        .unwrap();

    lobby_event(&mut host, LobbyStatus::Open, 2).await;
    lobby_event(&mut remaining, LobbyStatus::Open, 2).await;
    keepers[0]
        .keeper
        .close_lobby(lobby, LobbyRole::Host)
        .await
        .unwrap();
    lobby_event(&mut remaining, LobbyStatus::Closed, 2).await;
    assert_eq!(
        info(&keepers[1].keeper, lobby).await.status,
        LobbyStatus::Closed
    );
    assert!(
        keepers[0]
            .keeper
            .close_lobby(lobby, LobbyRole::Guest)
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lobby_admits_only_its_size_and_starts_only_when_full() {
    let harness = Harness::new(fast_config());
    let [host, grace, alan] = [harness.keeper(1), harness.keeper(2), harness.keeper(3)];
    let mut host_events = host.keeper.subscribe();
    let (lobby, invite) = host
        .keeper
        .open_lobby(LobbyOpen {
            members: 2,
            ..open_request("Ada")
        })
        .await
        .unwrap();
    let early = host.keeper.start_lobby(lobby, 2, 2, [7; 32], SECRETS).await;
    assert!(matches!(
        early,
        Err(ome_keeper::Error::Incomplete {
            joined: 1,
            expected: 2
        })
    ));
    grace
        .keeper
        .join_lobby(&invite, "Grace".into(), [0x02; 32], point(101))
        .await
        .unwrap();
    lobby_event(&mut host_events, LobbyStatus::Open, 2).await;
    alan.keeper
        .join_lobby(&invite, "Alan".into(), [0x03; 32], point(102))
        .await
        .unwrap();
    // The third join is refused: the roster stays at two, with each member's account.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let roster = info(&host.keeper, lobby).await;
    assert_eq!(roster.expected, 2);
    let accounts: Vec<[u8; 32]> = roster.members.iter().map(|m| m.account).collect();
    assert_eq!(accounts, [[0x01; 32], [0x02; 32]]);
    host.keeper
        .start_lobby(lobby, 2, 2, [7; 32], SECRETS)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_host_removes_a_guest_and_its_invite_stops_working() {
    let harness = Harness::new(fast_config());
    let (keepers, lobby, old_invite) = lobby_of_three(&harness).await;
    let mut removed = keepers[2].keeper.subscribe();
    let mut staying = keepers[1].keeper.subscribe();
    let removed_identity = keepers[2].keeper.identity();

    let after = keepers[0]
        .keeper
        .remove_from_lobby(lobby, removed_identity)
        .await
        .unwrap();

    assert_eq!(after.members.len(), 2);
    let new_invite = after.invite.unwrap();
    assert_ne!(new_invite, old_invite);
    lobby_event(&mut staying, LobbyStatus::Open, 2).await;
    wait_for(&mut removed, |e| match e {
        KeeperEvent::Lobby {
            status: LobbyStatus::Closed,
            ..
        } => Some(()),
        _ => None,
    })
    .await;
    assert!(info(&keepers[2].keeper, lobby).await.removed);

    // The old invite no longer admits anyone; the new one does.
    let latecomer = harness.keeper(4);
    let mut host_events = keepers[0].keeper.subscribe();
    latecomer
        .keeper
        .join_lobby(&old_invite, "Eve".into(), [0x04; 32], point(104))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(info(&keepers[0].keeper, lobby).await.members.len(), 2);
    let again = harness.keeper(5);
    again
        .keeper
        .join_lobby(&new_invite, "Hal".into(), [0x05; 32], point(105))
        .await
        .unwrap();
    lobby_event(&mut host_events, LobbyStatus::Open, 3).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_host_changes_the_size_and_thresholds_before_starting() {
    let harness = Harness::new(fast_config());
    let (keepers, lobby, _) = lobby_of_three(&harness).await;
    let mut guest = keepers[1].keeper.subscribe();
    // Smaller than the roster, or a threshold above the size, is refused.
    assert!(
        keepers[0]
            .keeper
            .update_lobby(lobby, Some(2), None, None)
            .await
            .is_err()
    );
    assert!(
        keepers[0]
            .keeper
            .update_lobby(lobby, None, Some(4), None)
            .await
            .is_err()
    );

    let updated = keepers[0]
        .keeper
        .update_lobby(lobby, Some(4), Some(3), Some(3))
        .await
        .unwrap();

    assert_eq!((updated.expected, updated.threshold), (4, 3));
    wait_for(&mut guest, |e| match e {
        KeeperEvent::Lobby {
            change: Some(change),
            ..
        } if change.kind == ome_api::keeper::LobbyChangeKind::Updated => Some(()),
        _ => None,
    })
    .await;
    let seen = info(&keepers[1].keeper, lobby).await;
    assert_eq!(
        (seen.expected, seen.threshold, seen.viewing_threshold),
        (4, 3, 3)
    );
}
