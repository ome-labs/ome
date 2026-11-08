//! The daemon client against a stand-in daemon that speaks the ome-api envelopes, and discovery of a daemon from the
//! Ome home.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::{ffi::OsString, os::unix::fs::PermissionsExt, path::Path, time::Duration};

use ome_api::{
    API_VERSION, ErrorKind, Method, RpcError,
    daemon::{
        DaemonEvent, DaemonInfo, DaemonInfoGet, Secret, SignerStatus, SignerUnlock, UnlockParams,
    },
    jsonrpc::{Notification, Request, Response},
};
use ome_client::{ADDR_FILE, ClientError, DaemonClient, Home, TOKEN_FILE};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

const TOKEN: &str = "daemon-token";

fn info() -> DaemonInfo {
    DaemonInfo {
        api_version: API_VERSION,
        version: "0.0.0".into(),
        pid: 42,
        profile: "alice".into(),
        profile_dir: "/tmp/alice".into(),
        started_at: 1_800_000_000,
        node: "http://localhost:8080".into(),
    }
}

/// Answers one method the way a daemon would.
fn answer(request: &Request) -> Result<Value, RpcError> {
    match request.method.as_str() {
        DaemonInfoGet::NAME => Ok(serde_json::to_value(info()).unwrap()),
        SignerUnlock::NAME => match request.params::<UnlockParams>() {
            Ok(UnlockParams {
                secret: Secret::Passphrase { passphrase },
                ttl_secs,
            }) if passphrase == "correct horse" => {
                let status = SignerStatus {
                    locked: false,
                    unlocked_until: Some(1_800_000_000 + u64::from(ttl_secs)),
                };
                Ok(serde_json::to_value(status).unwrap())
            }
            Ok(_) => Err(RpcError::new(ErrorKind::Auth, "wrong passphrase")),
            Err(e) => Err(RpcError::new(ErrorKind::Invalid, e.to_string())),
        },
        "hang.up" => Err(RpcError::new(ErrorKind::Failed, "unreachable")),
        other => Err(RpcError::new(
            ErrorKind::MethodNotFound,
            format!("unknown method {other}"),
        )),
    }
}

/// A stand-in daemon on `socket`: it checks the token, answers [`answer`], sends a signer event after
/// `events.subscribe`, and drops the connection on `hang.up`.
fn fake_daemon(socket: &Path) {
    let listener = UnixListener::bind(socket).unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let (reader, mut writer) = stream.into_split();
                let mut lines = BufReader::new(reader).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let request: Request = serde_json::from_str(&line).unwrap();
                    let id = request.id.clone();
                    let mut out = vec![];
                    match request.method.as_str() {
                        "session.auth" if request.params["token"] == TOKEN => {
                            out.push(json!(Response::ok(id, Value::Null)));
                        }
                        "session.auth" => out.push(json!(Response::err(
                            id,
                            RpcError::new(ErrorKind::Unauthorized, "invalid token")
                        ))),
                        "events.subscribe" => {
                            out.push(json!(Response::ok(id, Value::Null)));
                            let event = DaemonEvent::Signer {
                                signer: SignerStatus {
                                    locked: true,
                                    unlocked_until: None,
                                },
                            };
                            out.push(json!(Notification::event(event)));
                        }
                        "hang.up" => return,
                        _ => out.push(json!(match answer(&request) {
                            Ok(result) => Response::ok(id, result),
                            Err(error) => Response::err(id, error),
                        })),
                    }
                    for message in out {
                        writer
                            .write_all(format!("{message}\n").as_bytes())
                            .await
                            .unwrap();
                    }
                }
            });
        }
    });
}

/// A home whose files point at a running stand-in daemon.
fn home_with_daemon(dir: &Path) -> Home {
    fake_daemon(&dir.join("omed.sock"));
    std::fs::write(dir.join(ADDR_FILE), "omed.sock\n").unwrap();
    let token = dir.join(TOKEN_FILE);
    std::fs::write(&token, format!("{TOKEN}\n")).unwrap();
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
    Home::at(dir)
}

#[tokio::test]
async fn a_client_discovers_the_daemon_and_calls_it() {
    let dir = tempfile::tempdir().unwrap();
    let client = DaemonClient::discover(&home_with_daemon(dir.path()))
        .await
        .unwrap();

    assert_eq!(
        client
            .call::<DaemonInfoGet>(&ome_api::Empty {})
            .await
            .unwrap(),
        info()
    );
    let unlocked = client
        .call::<SignerUnlock>(&UnlockParams {
            secret: Secret::Passphrase {
                passphrase: "correct horse".into(),
            },
            ttl_secs: 60,
        })
        .await
        .unwrap();
    assert!(!unlocked.locked);
    let refused = client
        .call::<SignerUnlock>(&UnlockParams {
            secret: Secret::Passphrase {
                passphrase: "wrong".into(),
            },
            ttl_secs: 60,
        })
        .await
        .err()
        .unwrap();
    assert_eq!(refused.kind(), ErrorKind::Auth);

    let mut events = client.events().await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(5), events.next())
        .await
        .unwrap();
    assert!(matches!(
        event,
        Some(Ok(DaemonEvent::Signer {
            signer: SignerStatus { locked: true, .. }
        }))
    ));
}

/// Not a daemon method: the stand-in answers it by hanging up.
struct HangUp;

impl Method for HangUp {
    const NAME: &'static str = "hang.up";
    type Params = ome_api::Empty;
    type Result = ();
}

impl ome_api::daemon::DaemonMethod for HangUp {}

#[tokio::test]
async fn a_dropped_connection_fails_the_waiting_call() {
    let dir = tempfile::tempdir().unwrap();
    let client = DaemonClient::discover(&home_with_daemon(dir.path()))
        .await
        .unwrap();
    let closed = client
        .call::<HangUp>(&ome_api::Empty {})
        .await
        .err()
        .unwrap();
    assert!(matches!(closed, ClientError::Closed), "{closed}");
    assert!(matches!(
        client.call::<DaemonInfoGet>(&ome_api::Empty {}).await,
        Err(ClientError::Closed | ClientError::Io(_))
    ));
}

#[tokio::test]
async fn discovery_refuses_missing_files_and_a_shared_token() {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::at(dir.path());
    let missing = home.daemon().err().unwrap();
    assert_eq!(missing.kind(), ErrorKind::NotReady);

    std::fs::write(dir.path().join(ADDR_FILE), "omed.sock").unwrap();
    let token = dir.path().join(TOKEN_FILE);
    std::fs::write(&token, TOKEN).unwrap();
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        home.daemon()
            .err()
            .unwrap()
            .to_string()
            .contains("readable by other users")
    );

    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
    let endpoint = home.daemon().unwrap();
    assert_eq!(endpoint.token, TOKEN);
    assert!(!format!("{endpoint:?}").contains(TOKEN));
}

#[test]
fn the_home_is_ome_home_or_dot_ome() {
    let resolve = |ome: Option<&str>, user: Option<&str>| {
        Home::resolve(ome.map(OsString::from), user.map(OsString::from))
    };
    assert_eq!(
        resolve(Some("/srv/ome"), Some("/home/a")).unwrap(),
        Home::at("/srv/ome")
    );
    assert_eq!(
        resolve(None, Some("/home/a")).unwrap(),
        Home::at("/home/a/.ome")
    );
    assert_eq!(
        resolve(Some(""), Some("/home/a")).unwrap(),
        Home::at("/home/a/.ome")
    );
    assert!(resolve(None, None).is_err());
}
