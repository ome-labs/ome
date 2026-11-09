//! The keeper's RPC over the network, for a member whose keeper runs on their own server: TLS 1.3 with a self-signed
//! certificate. There is no certificate authority: the member's daemon pins the certificate's SHA-256 fingerprint,
//! which the keeper prints when it starts. The certificate and its key are made once and kept in custody, so the
//! fingerprint stays the same across restarts. Sessions still present the RPC token first.

use std::sync::Arc;

use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use super::server::{RpcToken, connection};
use crate::{Error, Keeper, Result, custody::SecretStore};

/// Custody slot of the certificate and its key.
const SLOT: &str = "rpc-tls";

#[derive(Serialize, Deserialize)]
struct Stored {
    certificate: Vec<u8>,
    key: Vec<u8>,
}

/// The keeper's TLS identity: a server configuration and the fingerprint daemons pin.
pub struct TlsIdentity {
    config: Arc<ServerConfig>,
    fingerprint: [u8; 32],
}

impl TlsIdentity {
    /// Loads the certificate from custody, or makes and stores one.
    ///
    /// # Errors
    /// Custody errors, or [`Error::Invalid`] if TLS cannot be set up.
    pub fn load_or_create(secrets: &SecretStore) -> Result<Self> {
        let stored = if secrets.exists(SLOT) {
            serde_json::from_slice::<Stored>(&secrets.get(SLOT)?)
                .map_err(|_| Error::Storage("the RPC certificate is damaged".into()))?
        } else {
            let generated = rcgen::generate_simple_self_signed(vec!["ome-keeper".to_owned()])
                .map_err(|e| Error::Invalid(format!("cannot make the RPC certificate: {e}")))?;
            let stored = Stored {
                certificate: generated.cert.der().to_vec(),
                key: generated.signing_key.serialize_der(),
            };
            let bytes = serde_json::to_vec(&stored).map_err(|e| Error::Storage(e.to_string()))?;
            secrets.put(SLOT, &bytes)?;
            stored
        };
        let fingerprint = Sha256::digest(&stored.certificate).into();
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|e| Error::Invalid(e.to_string()))?
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(stored.certificate)],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(stored.key)),
                )
                .map_err(|e| Error::Invalid(format!("the RPC certificate is unusable: {e}")))?;
        Ok(Self {
            config: Arc::new(config),
            fingerprint,
        })
    }

    /// The SHA-256 fingerprint of the certificate, as lowercase hex: what a daemon pins.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        hex::encode(self.fingerprint)
    }
}

/// Serves JSON-RPC 2.0 over TLS on `listener`.
///
/// # Errors
/// Accept failures.
pub async fn serve_tls(
    keeper: Keeper,
    listener: TcpListener,
    token: RpcToken,
    identity: TlsIdentity,
) -> Result<()> {
    let acceptor = TlsAcceptor::from(identity.config);
    tracing::info!(address = %listener.local_addr()?, "rpc listening over TLS");
    loop {
        let (stream, peer) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let keeper = keeper.clone();
        let token = token.clone();
        tokio::spawn(async move {
            match acceptor.accept(stream).await {
                Ok(stream) => {
                    let (reader, writer) = tokio::io::split(stream);
                    connection(keeper, token, reader, writer).await;
                }
                Err(error) => tracing::debug!(%peer, %error, "TLS handshake failed"),
            }
        });
    }
}
