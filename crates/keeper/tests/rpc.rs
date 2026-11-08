//! The local RPC over a real Unix socket: sessions authenticate first, methods answer for a live vault, and errors
//! come back as JSON-RPC errors.
#![cfg(unix)]
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use std::{os::unix::fs::PermissionsExt, path::Path};

use ome_keeper::rpc::{self, RpcToken};
use ome_wire::{Message, types::field};
use serde_json::{Value, json};
use support::{WAIT, body, fast_config, point, vault};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

const TOKEN: &str = "launch-token";

struct Client {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
    next: u64,
}

impl Client {
    /// Connects once the server listens: the socket file appears at `bind`, a moment before `listen`.
    async fn connect(path: &Path) -> Self {
        let stream = tokio::time::timeout(WAIT, async {
            loop {
                match UnixStream::connect(path).await {
                    Ok(stream) => return stream,
                    Err(_) => tokio::task::yield_now().await,
                }
            }
        })
        .await
        .unwrap();
        let (reader, writer) = stream.into_split();
        Self {
            reader: BufReader::new(reader),
            writer,
            next: 0,
        }
    }

    async fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let request =
            json!({ "jsonrpc": "2.0", "id": self.next, "method": method, "params": params });
        self.writer
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        loop {
            let mut line = String::new();
            tokio::time::timeout(WAIT, self.reader.read_line(&mut line))
                .await
                .unwrap()
                .unwrap();
            let value: Value = serde_json::from_str(&line).unwrap();
            if value.get("id") == Some(&json!(self.next)) {
                return value;
            }
        }
    }

    async fn ok(&mut self, method: &str, params: Value) -> Value {
        let reply = self.call(method, params).await;
        assert!(reply.get("error").is_none(), "{method}: {reply}");
        reply["result"].clone()
    }
}

/// Serves `keeper`'s RPC on a socket in `dir` and waits until it is bound.
async fn serve(keeper: &ome_keeper::Keeper, dir: &Path) -> std::path::PathBuf {
    let socket = dir.join("keeper.sock");
    tokio::spawn(rpc::serve(
        keeper.clone(),
        socket.clone(),
        RpcToken::new(TOKEN),
    ));
    tokio::time::timeout(WAIT, async {
        while !socket.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    socket
}

fn hex(bytes: &[u8]) -> String {
    format!("0x{}", ::hex::encode(bytes))
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_must_authenticate_first_and_the_socket_is_owner_only() {
    let v = vault(fast_config()).await;
    let socket = serve(&v.keepers[0].keeper, v.keepers[0].dir.path()).await;
    let mut client = Client::connect(&socket).await;

    assert_eq!(
        client.call("keeper.info", json!({})).await["error"]["code"],
        -32_001
    );
    assert_eq!(
        client
            .call("session.auth", json!({ "token": "wrong" }))
            .await["error"]["code"],
        -32_001
    );
    assert_eq!(
        client.ok("session.auth", json!({ "token": TOKEN })).await,
        json!(true)
    );
    let info = client.ok("keeper.info", json!({})).await;
    assert_eq!(info["identity"], hex(&v.keepers[0].keeper.identity().0));
    assert_eq!(
        std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_drives_a_live_vault_over_the_socket() {
    let v = vault(fast_config()).await;
    let socket = serve(&v.keepers[0].keeper, v.keepers[0].dir.path()).await;
    let mut client = Client::connect(&socket).await;
    client.ok("session.auth", json!({ "token": TOKEN })).await;
    let vault = hex(&v.address);

    let vaults = client.ok("vault.list", json!({})).await;
    assert_eq!(vaults[0]["serving"], json!(true));

    let p = point(3);
    let points = json!([{ "x": hex(&p.x), "y": hex(&p.y) }]);
    let ecdh = client
        .ok(
            "ecdh.batch",
            json!({ "vault": vault, "epoch": 0, "purpose": "decryption", "points": points }),
        )
        .await;
    assert_eq!(ecdh["responses"].as_array().unwrap().len(), 2);

    let config = &v.config;
    let observed = client
        .ok(
            "vault.observeConfig",
            json!({
                "vault": vault,
                "threshold": config.threshold,
                "signerCount": config.signer_count,
                "version": config.version,
                "signerSetCommitment": hex(&field(&config.signer_set_commitment)),
                "viewingThreshold": config.viewing_threshold,
                "committeeCommitment": hex(&field(&config.committee_commitment)),
            }),
        )
        .await;
    assert_eq!(observed["observation"], json!("current"));

    let (proposal, payload) = v.proposal(30);
    let hash = client
        .ok(
            "payloads.put",
            json!({ "vault": vault, "body": hex(&body(&Message::Payload(payload.clone()))) }),
        )
        .await;
    assert_eq!(hash["hash"], hex(&payload.hash()));
    let id = client
        .ok(
            "proposals.put",
            json!({ "vault": vault, "body": hex(&body(&Message::Proposal(proposal.clone()))) }),
        )
        .await;
    assert_eq!(id["id"], hex(&proposal.id().unwrap()));
    let fetched = client
        .ok(
            "payloads.get",
            json!({ "vault": vault, "hash": hex(&payload.hash()) }),
        )
        .await;
    assert_eq!(fetched, json!(hex(&body(&Message::Payload(payload)))));
    assert_eq!(
        client
            .ok("proposals.list", json!({ "vault": vault }))
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let audit = client
        .ok("audit.list", json!({ "vault": vault, "limit": 5 }))
        .await;
    assert_ne!(audit, json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn keeper_errors_and_unknown_methods_come_back_as_errors() {
    let v = vault(fast_config()).await;
    let socket = serve(&v.keepers[0].keeper, v.keepers[0].dir.path()).await;
    let mut client = Client::connect(&socket).await;
    client.ok("session.auth", json!({ "token": TOKEN })).await;

    // The chain still names this committee, so the share cannot be retired.
    let retire = client
        .call("vault.retire", json!({ "vault": hex(&v.address) }))
        .await;
    assert_eq!(retire["error"]["code"], -32_000);
    assert!(
        retire["error"]["message"]
            .as_str()
            .unwrap()
            .contains("has not moved past")
    );
    assert_eq!(
        client
            .call("vault.status", json!({ "vault": hex(&[9; 32]) }))
            .await["error"]["code"],
        -32_000
    );
    assert!(
        client
            .call("no.such", json!({}))
            .await
            .get("error")
            .is_some()
    );
}
