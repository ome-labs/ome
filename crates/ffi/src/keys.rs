//! The device's keys. Its Ed25519 identity (which signs every frame and is its libp2p peer id) and X25519 transport
//! key are sealed together under the transport sealing key; its Grumpkin signing key is an `ome-signer` sealed key
//! under the signing sealing key, opened for one signature only.

use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use ome_crypto::{
    Point,
    sealing::envelope::{TransportPublic, TransportSecret},
};
use ome_signer::{SealedKey, SealingKey, Secret, Signer, SigningKey};
use ome_wire::{IdentityPublic, KeeperIdentity, types::WirePoint};
use os_rng::SysRng;
use rand_core::{Rng, UnwrapErr};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    FfiError, KeyPurpose,
    error::Result,
    store::{Store, read_json, sealing_key, write_json},
};

const DEVICE_FILE: &str = "device.json";
const KEYS_FILE: &str = "keys.sealed";
const SIGNER_FILE: &str = "signer.json";
const FORMAT: &str = "ome.device.v0";
/// Associated data of the sealed identity and transport keys.
const KEYS_DOMAIN: &[u8] = b"ome.device.keys.v0";
const NONCE_BYTES: usize = 12;

pub(crate) fn rng() -> UnwrapErr<SysRng> {
    UnwrapErr(SysRng)
}

/// The public half of the device, kept in the clear.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeviceFile {
    v: String,
    pub(crate) name: String,
    pub(crate) identity: [u8; 32],
    pub(crate) transport: [u8; 32],
    pub(crate) signer_key: WirePoint,
}

/// The device's identity and transport keys, opened.
pub(crate) struct DeviceKeys {
    pub(crate) identity: KeeperIdentity,
    pub(crate) transport: TransportSecret,
    pub(crate) public: DeviceFile,
}

impl DeviceKeys {
    pub(crate) fn transport_public(&self) -> TransportPublic {
        TransportPublic(self.public.transport)
    }

    pub(crate) fn identity_public(&self) -> IdentityPublic {
        IdentityPublic(self.public.identity)
    }

    /// Creates the device's three keys and stores them sealed. Refuses if a device already exists.
    pub(crate) fn create(store: &Store, name: String) -> Result<Self> {
        if store.read(DEVICE_FILE.into())?.is_some() {
            return Err(FfiError::invalid("this store already holds a device"));
        }
        if name.len() > ome_wire::limits::MAX_NAME_BYTES {
            return Err(FfiError::invalid("a device name is at most 64 bytes"));
        }
        let mut rng = rng();
        let identity = KeeperIdentity::generate(&mut rng);
        let (transport, transport_public) = TransportSecret::generate(&mut rng);
        let signer = SigningKey::generate(&mut rng)?;
        let signing_key = SealingKey::from_bytes(*sealing_key(store, KeyPurpose::Signing)?);
        let sealed_signer = SealedKey::seal(&signer, &Secret::Key(&signing_key), &mut rng)?;
        let public = DeviceFile {
            v: FORMAT.into(),
            name,
            identity: identity.public().0,
            transport: transport_public.0,
            signer_key: WirePoint::from(&signer.public_key()),
        };
        let mut secrets = Zeroizing::new([0u8; 64]);
        secrets[..32].copy_from_slice(identity.seed().as_slice());
        secrets[32..].copy_from_slice(transport.to_bytes().as_slice());
        let mut nonce = [0u8; NONCE_BYTES];
        rng.fill_bytes(&mut nonce);
        let sealing = sealing_key(store, KeyPurpose::Transport)?;
        let ciphertext = cipher(&sealing)
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: secrets.as_slice(),
                    aad: &associated_data(&public),
                },
            )
            .map_err(|_| FfiError::storage("cannot seal the device keys"))?;
        let mut sealed = nonce.to_vec();
        sealed.extend_from_slice(&ciphertext);
        let json = sealed_signer
            .to_json()
            .map_err(|e| FfiError::storage(e.to_string()))?;
        store.write(SIGNER_FILE.into(), json.into_bytes())?;
        store.write(KEYS_FILE.into(), sealed)?;
        write_json(store, DEVICE_FILE, &public)?;
        Ok(Self {
            identity,
            transport,
            public,
        })
    }

    /// Opens the device's identity and transport keys with the transport sealing key.
    pub(crate) fn open(store: &Store) -> Result<Self> {
        let public: DeviceFile = read_json(store, DEVICE_FILE)?
            .ok_or_else(|| FfiError::not_paired("this store holds no device yet"))?;
        if public.v != FORMAT {
            return Err(FfiError::storage(format!(
                "unsupported device format {}",
                public.v
            )));
        }
        let sealed = store
            .read(KEYS_FILE.into())?
            .ok_or_else(|| FfiError::storage("the device keys are missing"))?;
        if sealed.len() <= NONCE_BYTES {
            return Err(FfiError::storage("the device keys are damaged"));
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_BYTES);
        let mut fixed = [0u8; NONCE_BYTES];
        fixed.copy_from_slice(nonce);
        let sealing = sealing_key(store, KeyPurpose::Transport)?;
        let secrets = Zeroizing::new(
            cipher(&sealing)
                .decrypt(
                    &Nonce::from(fixed),
                    Payload {
                        msg: ciphertext,
                        aad: &associated_data(&public),
                    },
                )
                .map_err(|_| FfiError::Denied {
                    reason: "the device keys do not open with this sealing key".into(),
                })?,
        );
        let mut seed = Zeroizing::new([0u8; 32]);
        seed.copy_from_slice(&secrets[..32]);
        let mut secret = [0u8; 32];
        secret.copy_from_slice(&secrets[32..64]);
        let identity = KeeperIdentity::from_seed(&seed);
        let transport = TransportSecret::from_bytes(secret);
        secret.fill(0);
        if identity.public().0 != public.identity || transport.public()?.0 != public.transport {
            return Err(FfiError::storage(
                "the device keys do not match its public file",
            ));
        }
        Ok(Self {
            identity,
            transport,
            public,
        })
    }

    /// The signing key's public half.
    pub(crate) fn signer_key(&self) -> Result<Point> {
        Ok(Point::try_from(&self.public.signer_key)?)
    }

    /// Opens the signing key with the signing sealing key, which makes the app ask the user.
    pub(crate) fn signing_key(store: &Store) -> Result<SigningKey> {
        let json = store
            .read(SIGNER_FILE.into())?
            .ok_or_else(|| FfiError::storage("the signing key is missing"))?;
        let text =
            String::from_utf8(json).map_err(|_| FfiError::storage("the signing key is damaged"))?;
        let sealed = SealedKey::from_json(&text)?;
        let key = SealingKey::from_bytes(*sealing_key(store, KeyPurpose::Signing)?);
        Ok(sealed.open(&Secret::Key(&key))?)
    }
}

fn cipher(key: &[u8; 32]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(&Key::from(*key))
}

/// Binds the sealed keys to the device's public keys, so a file cannot be paired with another device's keys.
fn associated_data(public: &DeviceFile) -> Vec<u8> {
    let mut aad = KEYS_DOMAIN.to_vec();
    aad.extend_from_slice(&public.identity);
    aad.extend_from_slice(&public.transport);
    aad
}
