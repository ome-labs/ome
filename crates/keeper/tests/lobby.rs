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
        network: "local-network".into(),
        member_name: name.into(),
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
                .join_lobby(&invite, name.into(), point(seed))
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
        .join_lobby(&forged.encode().unwrap(), "Mallory".into(), point(103))
        .await
        .unwrap();
    honest
        .keeper
        .join_lobby(&invite, "Grace".into(), point(101))
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
