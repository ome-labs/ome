//! Sealed secrets at rest: owner-only files, a wrong master key caught before any secret, tampered or swapped files
//! refused, destroyed secrets gone, and keys and shares surviving a restart.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

use ome_crypto::threshold::dkg;
use ome_keeper::{
    Error,
    custody::{KeeperKeys, MasterKey, SecretStore, ShareSlot, StoredShare},
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

#[test]
fn secrets_round_trip_and_never_appear_in_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::open(dir.path(), &MasterKey::from_bytes([1; 32])).unwrap();
    store.put("example", b"top secret value").unwrap();
    assert_eq!(
        store.get("example").unwrap().as_slice(),
        b"top secret value"
    );
    let raw = std::fs::read(dir.path().join("example.sealed")).unwrap();
    assert!(!raw.windows(10).any(|w| w == b"top secret"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("example.sealed"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn a_wrong_master_key_is_detected_before_any_secret() {
    let dir = tempfile::tempdir().unwrap();
    SecretStore::open(dir.path(), &MasterKey::from_bytes([1; 32])).unwrap();
    assert!(matches!(
        SecretStore::open(dir.path(), &MasterKey::from_bytes([2; 32])),
        Err(Error::Unseal(_))
    ));
}

#[test]
fn tampered_or_swapped_files_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::open(dir.path(), &MasterKey::from_bytes([1; 32])).unwrap();
    store.put("a", b"alpha").unwrap();
    store.put("b", b"beta").unwrap();
    // A file moved under another name does not open: the name is authenticated.
    std::fs::copy(dir.path().join("a.sealed"), dir.path().join("b.sealed")).unwrap();
    assert!(matches!(store.get("b"), Err(Error::Unseal(_))));
    let mut raw = std::fs::read(dir.path().join("a.sealed")).unwrap();
    let last = raw.len() - 1;
    raw[last] ^= 1;
    std::fs::write(dir.path().join("a.sealed"), raw).unwrap();
    assert!(matches!(store.get("a"), Err(Error::Unseal(_))));
}

#[test]
fn destroyed_secrets_are_gone_and_renames_rebind_the_name() {
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::open(dir.path(), &MasterKey::from_bytes([1; 32])).unwrap();
    store.put("pending-x", b"share").unwrap();
    store.rename("pending-x", "share-x-1").unwrap();
    assert!(!store.exists("pending-x"));
    assert_eq!(store.get("share-x-1").unwrap().as_slice(), b"share");
    store.destroy("share-x-1").unwrap();
    assert!(matches!(
        store.get("share-x-1"),
        Err(Error::MissingSecret(_))
    ));
    assert_eq!(store.list("share-").unwrap(), Vec::<String>::new());
}

#[test]
fn keys_and_shares_survive_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let key = || MasterKey::from_bytes([3; 32]);
    let (identity, transport) = {
        let store = SecretStore::open(dir.path(), &key()).unwrap();
        let keys = KeeperKeys::load_or_create(&store).unwrap();
        (keys.identity.public(), keys.transport_public)
    };
    let store = SecretStore::open(dir.path(), &key()).unwrap();
    let keys = KeeperKeys::load_or_create(&store).unwrap();
    assert_eq!(keys.identity.public(), identity);
    assert_eq!(keys.transport_public, transport);

    let mut rng = ChaCha20Rng::seed_from_u64(1);
    let params = dkg::Parameters {
        session: [0; 32],
        threshold: 2,
        participants: vec![1, 2],
    };
    let (s1, r1) = dkg::round1(&params, 1, &mut rng).unwrap();
    let (s2, r2) = dkg::round1(&params, 2, &mut rng).unwrap();
    let round1 = [r1, r2];
    let to_one = dkg::round2(&params, &s2, &round1).unwrap();
    let share = dkg::finalize(&params, s1, &round1, &to_one).unwrap();
    StoredShare::save(&store, ShareSlot::Active([9; 32], 0), &share).unwrap();
    let loaded = StoredShare::load(&store, ShareSlot::Active([9; 32], 0)).unwrap();
    assert_eq!(loaded.group_public_key(), share.group_public_key());
    assert_eq!(loaded.index(), 1);
}

#[test]
fn passphrase_keys_depend_on_the_salt() {
    let a = MasterKey::from_passphrase(b"correct horse", &[1; 16]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    SecretStore::open(dir.path(), &a).unwrap();
    let same = MasterKey::from_passphrase(b"correct horse", &[1; 16]).unwrap();
    assert!(SecretStore::open(dir.path(), &same).is_ok());
    let other_salt = MasterKey::from_passphrase(b"correct horse", &[2; 16]).unwrap();
    assert!(SecretStore::open(dir.path(), &other_salt).is_err());
    assert!(MasterKey::from_passphrase(b"", &[1; 16]).is_err());
}
