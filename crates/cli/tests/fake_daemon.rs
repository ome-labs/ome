//! The `ome` binary against a stand-in daemon that speaks the real socket protocol: output modes, references, exit
//! codes, and that a vote carries the passphrase only from where it may come.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};

use ome_api::{
    Amount, ErrorKind, Hex32, Method, RpcError,
    daemon::{
        ApproveParams, AztecAliases, Intent, ProposalStatus, ProposalView, ProposalsApprove,
        ProposalsList, ProposalsReview, Proposer, Review, ReviewChecks, Secret, SignerStatus,
        SignerStatusGet, Signs, Simulation, TokenInfo, VaultList, VaultSummary, Vote,
    },
    jsonrpc::{Request, Response},
};
use ome_client::{ADDR_FILE, TOKEN_FILE};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

const TOKEN: &str = "daemon-token";

fn hex(prefix: &str) -> Hex32 {
    format!("0x{prefix:0<64}").parse().unwrap()
}

fn vault() -> VaultSummary {
    VaultSummary {
        address: hex("7a11"),
        name: "Treasury".into(),
        alias: Some("treasury".into()),
        is_default: true,
        threshold: 2,
        viewing_threshold: 2,
        members: 3,
        my_index: 2,
        founded_at: 1_800_000_000,
    }
}

fn proposal(id: &str, status: ProposalStatus) -> ProposalView {
    ProposalView {
        id: hex(id),
        vault: vault().address,
        intent: Intent::Transfer {
            token: hex("05dc"),
            to: hex("b0b"),
            amount: Amount(250_000_000),
            memo: None,
        },
        proposer: Proposer {
            index: 1,
            name: "Alice".into(),
        },
        status,
        threshold: 2,
        members: 3,
        approvals: vec![Vote {
            slot: 0,
            index: 1,
            name: "Alice".into(),
            valid: true,
            reason: None,
        }],
        rejections: vec![],
        my_slot: 1,
        approved_by_me: false,
        rejected_by_me: false,
        config_version: 0,
        current_config_version: 0,
        expires_at: 4_000_000_000,
        tx_nonce: hex("7"),
        outer_hash: hex("91c3"),
        approval_message: hex("0e7a"),
        fee_method: "external".into(),
        calls: vec![],
        token: Some(TokenInfo {
            address: hex("05dc"),
            name: "USD Coin".into(),
            symbol: "USDC".into(),
            decimals: 6,
        }),
        execution: None,
        cancelled_by: None,
    }
}

fn review(p: ProposalView) -> Review {
    Review {
        vault_name: "Treasury".into(),
        my_name: "Bob".into(),
        checks: ReviewChecks {
            rebuilt: true,
            outer_hash: p.outer_hash,
            config_current: true,
            not_expired: true,
            failure: None,
        },
        action: "transfer 250 USDC".into(),
        recipient: None,
        simulation: Simulation {
            ok: true,
            gas: None,
            error: None,
        },
        signs: Signs {
            message: p.approval_message,
            formula: "Poseidon2(…)".into(),
        },
        rejects: Signs {
            message: hex("5e1e"),
            formula: "Poseidon2(…)".into(),
        },
        proposal: p,
    }
}

/// What the stand-in serves, and the approvals it was sent.
struct Daemon {
    proposals: Vec<ProposalView>,
    approvals: Mutex<Vec<ApproveParams>>,
}

#[allow(clippy::unnecessary_wraps, reason = "every answer is a result")]
fn to(value: impl serde::Serialize) -> Result<Value, RpcError> {
    Ok(serde_json::to_value(value).unwrap())
}

fn answer(daemon: &Daemon, request: &Request) -> Result<Value, RpcError> {
    match request.method.as_str() {
        VaultList::NAME => to(vec![vault()]),
        ProposalsList::NAME => to(daemon.proposals.clone()),
        SignerStatusGet::NAME => to(SignerStatus {
            locked: true,
            unlocked_until: None,
        }),
        AztecAliases::NAME => to(Vec::<()>::new()),
        ProposalsReview::NAME => to(review(daemon.proposals[0].clone())),
        ProposalsApprove::NAME => {
            let params: ApproveParams = request.params().unwrap();
            if params.secret.is_none() {
                return Err(RpcError::new(ErrorKind::Locked, "the signer is locked"));
            }
            daemon.approvals.lock().unwrap().push(params);
            let mut view = daemon.proposals[0].clone();
            view.status = ProposalStatus::Ready;
            view.approved_by_me = true;
            to(view)
        }
        other => Err(RpcError::new(
            ErrorKind::MethodNotFound,
            format!("unknown method {other}"),
        )),
    }
}

/// Starts the stand-in under `<home>/profiles/bob`, with the files a client discovers it by.
fn serve(home: &Path, daemon: Arc<Daemon>) {
    let dir = home.join("profiles/bob");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("profile.json"), "{}").unwrap();
    let listener = UnixListener::bind(dir.join("omed.sock")).unwrap();
    std::fs::write(dir.join(ADDR_FILE), "omed.sock\n").unwrap();
    std::fs::write(dir.join(TOKEN_FILE), format!("{TOKEN}\n")).unwrap();
    std::fs::set_permissions(dir.join(TOKEN_FILE), std::fs::Permissions::from_mode(0o600)).unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let daemon = daemon.clone();
            tokio::spawn(async move {
                let (reader, mut writer) = stream.into_split();
                let mut lines = BufReader::new(reader).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let request: Request = serde_json::from_str(&line).unwrap();
                    let id = request.id.clone();
                    let reply = match request.method.as_str() {
                        "session.auth" if request.params["token"] == TOKEN => {
                            Response::ok(id, Value::Null)
                        }
                        _ => match answer(&daemon, &request) {
                            Ok(result) => Response::ok(id, result),
                            Err(error) => Response::err(id, error),
                        },
                    };
                    writer
                        .write_all(format!("{}\n", json!(reply)).as_bytes())
                        .await
                        .unwrap();
                }
            });
        }
    });
}

struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}

async fn ome(home: &Path, args: &[&str], envs: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ome"));
    command
        .args(["--profile", "bob"])
        .args(args)
        .env("OME_HOME", home)
        .env_remove("OME_PASSPHRASE_FILE")
        .env_remove("OME_VAULT");
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap();
    Output {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

fn setup(proposals: Vec<ProposalView>) -> (tempfile::TempDir, Arc<Daemon>) {
    let home = tempfile::tempdir().unwrap();
    let daemon = Arc::new(Daemon {
        proposals,
        approvals: Mutex::new(vec![]),
    });
    serve(home.path(), daemon.clone());
    (home, daemon)
}

#[tokio::test]
async fn lists_proposals_as_json_and_plain_records() {
    let (home, _) = setup(vec![proposal("4f2a", ProposalStatus::Collecting)]);
    let json = ome(home.path(), &["--json", "proposals"], &[]).await;
    assert_eq!(json.code, 0, "{}", json.stderr);
    let listed: Vec<ProposalView> = serde_json::from_str(&json.stdout).unwrap();
    assert_eq!(listed[0].id, hex("4f2a"));

    let plain = ome(home.path(), &["--plain", "proposals"], &[]).await;
    let fields: Vec<&str> = plain.stdout.trim().split('\t').collect();
    assert_eq!(
        fields[..3],
        ["4f2a0000", "pay 250 USDC → 0xb0b0…0000", "1/2"]
    );
}

#[tokio::test]
async fn an_approval_takes_the_passphrase_from_the_file_only() {
    let (home, daemon) = setup(vec![proposal("4f2a", ProposalStatus::Collecting)]);
    let file = home.path().join("passphrase");
    std::fs::write(&file, "correct horse\n").unwrap();

    // Locked, no terminal and no file: refused before anything is signed, as `locked`.
    let refused = ome(home.path(), &["--json", "approve", "4f2a", "--yes"], &[]).await;
    assert_eq!(refused.code, 1);
    let error: Value = serde_json::from_str(refused.stderr.trim()).unwrap();
    assert_eq!(error["error"]["code"], "locked");
    assert!(daemon.approvals.lock().unwrap().is_empty());

    let approved = ome(
        home.path(),
        &["--json", "approve", "4f2a", "--yes"],
        &[("OME_PASSPHRASE_FILE", &file)],
    )
    .await;
    assert_eq!(approved.code, 0, "{}", approved.stderr);
    let view: ProposalView = serde_json::from_str(&approved.stdout).unwrap();
    assert_eq!(view.status, ProposalStatus::Ready);
    let sent = daemon.approvals.lock().unwrap();
    assert_eq!(
        sent[0].secret,
        Some(Secret::Passphrase {
            passphrase: "correct horse".into()
        })
    );
}

#[tokio::test]
async fn failures_exit_by_kind_and_name_the_fix() {
    let (home, _) = setup(vec![
        proposal("4f2a", ProposalStatus::Collecting),
        proposal("4f2b", ProposalStatus::Collecting),
    ]);
    let early = ome(home.path(), &["execute", "4f2a"], &[]).await;
    assert_eq!(early.code, 4);
    assert!(
        early
            .stderr
            .contains("Can't execute 4f2a0000: it has 1 of 2 approvals.")
    );
    assert!(early.stderr.contains("ome approve 4f2a0000"));

    let ambiguous = ome(home.path(), &["proposal", "show", "4f2"], &[]).await;
    assert_eq!(ambiguous.code, 2);
    assert!(ambiguous.stderr.contains("4f2a00000000") && ambiguous.stderr.contains("4f2b00000000"));

    let missing = ome(home.path(), &["proposal", "show", "ffff"], &[]).await;
    assert_eq!(missing.code, 2);

    let unnamed = ome(home.path(), &["approve", "--no-input"], &[]).await;
    assert_eq!(unnamed.code, 2);
    assert!(unnamed.stderr.contains("<proposal>"), "{}", unnamed.stderr);
}
