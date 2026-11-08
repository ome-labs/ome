//! The health endpoint answers, and its metrics carry aggregates only: no peer, mailbox or envelope id.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use ome_net::{Libp2pTransport, Mailbox, RelayMailbox};
use ome_relay::health;
use ome_wire::messages::mailbox::RelayEnvelope;
use support::{identity, node, relay};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

async fn get(address: std::net::SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(format!("GET {path} HTTP/1.1\r\nhost: relay\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

#[tokio::test]
async fn health_and_metrics_answer_without_identifying_anyone() {
    let (mut relay, address) = relay(9, None).await;
    let served = health::serve(
        "127.0.0.1:0".parse().unwrap(),
        relay.peer_id.to_string(),
        Arc::clone(&relay.store),
        Arc::clone(&relay.metrics),
    )
    .await
    .unwrap();
    let (keeper, _) =
        Libp2pTransport::start(&identity(1), node(&["/ip4/127.0.0.1/tcp/0"], vec![address]))
            .await
            .unwrap();
    let envelope = RelayEnvelope {
        recipient: [7; 32],
        expires_at: support::in_an_hour(),
        sealed: vec![1; 40],
    };
    let id = envelope.id().unwrap();
    RelayMailbox::new(keeper.clone())
        .deposit(envelope)
        .await
        .unwrap();

    let health = get(served, "/health").await;
    let metrics = get(served, "/metrics").await;

    assert!(health.starts_with("HTTP/1.1 200 OK"), "{health}");
    assert!(health.contains("\"envelopes\":1"), "{health}");
    assert!(metrics.contains("ome_relay_deposits_total 1"), "{metrics}");
    assert!(
        metrics.contains("ome_relay_refused_total{reason=\"quota\"} 0"),
        "{metrics}"
    );
    for secret in [
        hex::encode([7u8; 32]),
        hex::encode(id),
        keeper.peer_id().to_string(),
    ] {
        assert!(!metrics.contains(&secret), "metrics name {secret}");
    }
    assert!(get(served, "/nothing").await.starts_with("HTTP/1.1 404"));
    keeper.stop();
    relay.stop();
}
