//! The RPC over TLS, for a daemon on another machine: a client that pins the keeper's certificate fingerprint
//! authenticates and calls methods; a client pinning another fingerprint never gets a connection; and the
//! certificate, so its fingerprint, survives a restart.
#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use ome_keeper::{
    custody::{MasterKey, SecretStore},
    rpc::{self, RpcToken, TlsIdentity},
};
use rustls::{
    ClientConfig, DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::{Harness, WAIT, fast_config};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_rustls::TlsConnector;

/// Accepts exactly the certificate whose SHA-256 is `fingerprint`, as the daemon does.
#[derive(Debug)]
struct Pinned(String);

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if hex::encode(Sha256::digest(end_entity.as_ref())) == self.0 {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "certificate fingerprint mismatch".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not used".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

async fn call(
    address: std::net::SocketAddr,
    fingerprint: &str,
    calls: &[(&str, Value)],
) -> Result<Vec<Value>, String> {
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pinned(fingerprint.to_owned())))
            .with_no_client_auth();
    let tcp = tokio::net::TcpStream::connect(address).await.unwrap();
    let stream = TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from("ome-keeper").unwrap(), tcp)
        .await
        .map_err(|e| e.to_string())?;
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut out = Vec::new();
    for (id, (method, params)) in calls.iter().enumerate() {
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writer
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        let mut line = String::new();
        tokio::time::timeout(WAIT, reader.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        out.push(serde_json::from_str(&line).unwrap());
    }
    Ok(out)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pinned_client_calls_the_keeper_over_tls_and_another_fingerprint_is_refused() {
    let harness = Harness::new(fast_config());
    let keeper = harness.keeper(1);
    let dir = tempfile::tempdir().unwrap();
    let secrets =
        SecretStore::open(dir.path().join("secrets"), &MasterKey::from_bytes([7; 32])).unwrap();
    let identity = TlsIdentity::load_or_create(&secrets).unwrap();
    let fingerprint = identity.fingerprint();
    assert_eq!(
        TlsIdentity::load_or_create(&secrets).unwrap().fingerprint(),
        fingerprint
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(rpc::serve_tls(
        keeper.keeper.clone(),
        listener,
        RpcToken::new("token"),
        identity,
    ));

    let replies = call(
        address,
        &fingerprint,
        &[
            ("session.auth", json!({ "token": "token" })),
            ("keeper.info", json!({})),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        replies[1]["result"]["identity"],
        json!(format!("0x{}", hex::encode(keeper.keeper.identity().0)))
    );
    let refused = call(address, &"00".repeat(32), &[("keeper.info", json!({}))]).await;
    assert!(refused.unwrap_err().contains("fingerprint"));
}
