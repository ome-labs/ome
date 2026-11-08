//! The keeper client against real keepers on the in-memory network, each serving its RPC on a socket: two members run
//! a DKG through typed calls and see it complete on their event streams.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use ome_api::{
    Empty, ErrorKind, Hex32,
    keeper::{
        CeremonyDkg, CeremonyResult, CeremonyStatus, CeremonyStatusGet, DkgParams, KeeperContact,
        KeeperEvent, KeeperInfoGet, SessionParams, VaultList, VaultParams, VaultStatusGet,
    },
};
use ome_client::{ClientError, KeeperClient};
use ome_keeper::{
    Keeper, KeeperConfig, KeeperParts, SystemClock,
    custody::{KeeperKeys, MasterKey, SecretStore},
    rpc::{RpcToken, serve},
    store::Store,
};
use ome_net::MemoryNetwork;
use tempfile::TempDir;

const TOKEN: &str = "launch-token";
const WAIT: Duration = Duration::from_secs(20);

struct Served {
    _dir: TempDir,
    socket: PathBuf,
}

/// Starts a keeper on `net` and serves its RPC.
fn keeper(net: &Arc<MemoryNetwork>, seed: u8) -> Served {
    let dir = tempfile::tempdir().unwrap();
    let secrets = SecretStore::open(
        dir.path().join("secrets"),
        &MasterKey::from_bytes([seed; 32]),
    )
    .unwrap();
    let keys = KeeperKeys::load_or_create(&secrets).unwrap();
    let store = Store::open(&dir.path().join("keeper.db")).unwrap();
    let (transport, inbound) = net.endpoint(keys.identity.public());
    let keeper = Keeper::start(KeeperParts {
        keys,
        secrets,
        store,
        transport: Box::new(transport),
        inbound,
        clock: Arc::new(SystemClock),
        config: KeeperConfig::default(),
        addresses: Vec::new(),
        mailbox: None,
        relays: Vec::new(),
    })
    .unwrap();
    let socket = dir.path().join("keeper.sock");
    tokio::spawn(serve(keeper, socket.clone(), RpcToken::new(TOKEN)));
    Served { _dir: dir, socket }
}

/// Connects once the server listens: the socket file appears at `bind`, a moment before `listen`.
async fn connect(served: &Served) -> KeeperClient {
    tokio::time::timeout(WAIT, async {
        loop {
            match KeeperClient::connect(&served.socket, TOKEN).await {
                Ok(client) => return client,
                Err(ClientError::Connect { .. }) => {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                Err(error) => panic!("{error}"),
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_token_is_refused() {
    let net = MemoryNetwork::new();
    let served = keeper(&net, 1);
    connect(&served).await;
    let refused = KeeperClient::connect(&served.socket, "wrong")
        .await
        .err()
        .unwrap();
    assert_eq!(refused.kind(), ErrorKind::Unauthorized);
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_carry_their_kind() {
    let net = MemoryNetwork::new();
    let client = connect(&keeper(&net, 1)).await;
    let unknown = client
        .call::<VaultStatusGet>(&VaultParams {
            vault: Hex32([9; 32]),
        })
        .await
        .err()
        .unwrap();
    assert_eq!(unknown.kind(), ErrorKind::NotFound);
    let missing = client
        .call::<CeremonyStatusGet>(&SessionParams {
            session: Hex32([9; 32]),
        })
        .await
        .err()
        .unwrap();
    assert_eq!(missing.kind(), ErrorKind::NotFound);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_members_run_a_dkg_through_their_clients() {
    let net = MemoryNetwork::new();
    let (a, b) = (keeper(&net, 1), keeper(&net, 2));
    let clients = [connect(&a).await, connect(&b).await];

    // Concurrent calls on one session each get their own answer.
    let (info_a, vaults) = tokio::join!(
        clients[0].call::<KeeperInfoGet>(&Empty {}),
        clients[0].call::<VaultList>(&Empty {})
    );
    let info_a = info_a.unwrap();
    assert_eq!(info_a.api_version, ome_api::API_VERSION);
    assert_eq!(vaults.unwrap(), Vec::new());
    let info_b = clients[1].call::<KeeperInfoGet>(&Empty {}).await.unwrap();

    let members: Vec<KeeperContact> = [&info_a, &info_b]
        .iter()
        .map(|info| KeeperContact {
            identity: info.identity,
            transport: info.transport,
            addresses: Vec::new(),
        })
        .collect();
    let params = DkgParams {
        session: Hex32([7; 32]),
        threshold: 2,
        members,
    };
    let mut streams = Vec::new();
    for client in &clients {
        streams.push(client.events().await.unwrap());
    }
    for client in &clients {
        client.call::<CeremonyDkg>(&params).await.unwrap();
    }
    for events in &mut streams {
        tokio::time::timeout(WAIT, async {
            loop {
                if let KeeperEvent::Ceremony {
                    session,
                    status: CeremonyStatus::Completed,
                    ..
                } = events.next().await.unwrap().unwrap()
                {
                    assert_eq!(session, params.session);
                    return;
                }
            }
        })
        .await
        .unwrap();
    }

    let mut transcripts = Vec::new();
    for client in &clients {
        let ceremony = client
            .call::<CeremonyStatusGet>(&SessionParams {
                session: params.session,
            })
            .await
            .unwrap();
        assert_eq!(ceremony.status, CeremonyStatus::Completed);
        let Some(CeremonyResult::Dkg(result)) = ceremony.result else {
            panic!("no DKG result: {ceremony:?}");
        };
        assert_eq!(result.verification_shares.len(), 2);
        transcripts.push(result.transcript);
    }
    assert_eq!(transcripts[0], transcripts[1]);
}
