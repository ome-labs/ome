//! Minimal HTTP endpoint: `GET /health` (JSON) and `GET /metrics` (Prometheus).

use std::{net::SocketAddr, sync::Arc};

use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use crate::{Result, metrics::Metrics, store::MailboxStore};

/// Serves health and metrics on `addr` until the task is dropped.
///
/// # Errors
/// Bind failures.
pub async fn serve(
    addr: SocketAddr,
    peer_id: String,
    store: Arc<MailboxStore>,
    metrics: Arc<Metrics>,
) -> Result<SocketAddr> {
    let listener = TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let (store, metrics, peer_id) =
                (Arc::clone(&store), Arc::clone(&metrics), peer_id.clone());
            tokio::spawn(async move {
                let mut request = vec![0u8; 2048];
                let Ok(n) = stream.read(&mut request).await else {
                    return;
                };
                let line = String::from_utf8_lossy(&request[..n]);
                let path = line.split_whitespace().nth(1).unwrap_or("/");
                let usage = store.usage().unwrap_or_default();
                let (status, kind, body) = match path {
                    "/health" => (
                        "200 OK",
                        "application/json",
                        json!({ "status": "ok", "peerId": peer_id, "envelopes": usage.envelopes, "bytes": usage.bytes })
                            .to_string(),
                    ),
                    "/metrics" => ("200 OK", "text/plain; version=0.0.4", metrics.render(usage)),
                    _ => ("404 Not Found", "text/plain", "not found\n".to_owned()),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    Ok(local)
}
