//! The keeper's store across releases: a store a newer release migrated is refused instead of misread, and each
//! record keeps the protocol version it was stored in, which reads convert from (an unknown one fails clearly).
#![allow(clippy::unwrap_used)]

use ome_keeper::{Error, store::Store};
use rusqlite::Connection;

#[test]
fn a_store_from_a_newer_release_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keeper.sqlite");
    drop(Store::open(&path).unwrap());
    let known: i64 = Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", known + 1)
        .unwrap();

    match Store::open(&path) {
        Err(Error::NewerStore { found, known: k }) => {
            assert_eq!(
                (found, k),
                (
                    usize::try_from(known).unwrap() + 1,
                    usize::try_from(known).unwrap()
                )
            );
        }
        other => panic!("expected a refusal, got {:?}", other.err()),
    }
}

#[test]
fn records_are_read_in_the_version_they_were_stored_in() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keeper.sqlite");
    let store = Store::open(&path).unwrap();
    let vault = [7; 32];
    store
        .put_name(&vault, "vault", &[1; 32], 10, &[0xab])
        .unwrap();
    // Stored in the current version, a record reads back as written.
    assert_eq!(store.names(&vault).unwrap(), vec![vec![0xab]]);
    drop(store);

    // A record a future release stored in a version this one doesn't speak is refused on read, never misread.
    Connection::open(&path)
        .unwrap()
        .execute("UPDATE names SET version = 9", [])
        .unwrap();
    let store = Store::open(&path).unwrap();
    let error = store.names(&vault).unwrap_err().to_string();
    assert!(error.contains("unsupported protocol version 9"), "{error}");
}
