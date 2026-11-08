//! Mailbox storage: deposit, fetch and delete per mailbox; limits refused with their reason; expiry; bounded fetch
//! answers; and a database that records no depositor.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

use ome_relay::{Limits, store::MailboxStore};
use ome_wire::{
    limits::{MAX_ENVELOPE_BYTES, MAX_FETCH_ENVELOPES},
    messages::mailbox::{Refusal, RelayEnvelope},
};

const NOW: u64 = 1_800_000_000;

fn envelope(mailbox: u8, byte: u8, size: usize) -> RelayEnvelope {
    RelayEnvelope {
        recipient: [mailbox; 32],
        expires_at: NOW + 3600,
        sealed: vec![byte; size],
    }
}

#[test]
fn envelopes_are_stored_fetched_by_mailbox_and_deleted() {
    let store = MailboxStore::in_memory(Limits::default()).unwrap();
    let a = store.deposit(&envelope(1, 1, 40), NOW).unwrap().unwrap();
    let b = store.deposit(&envelope(1, 2, 40), NOW).unwrap().unwrap();
    store.deposit(&envelope(2, 3, 40), NOW).unwrap().unwrap();
    // Re-depositing the same envelope is idempotent.
    assert_eq!(store.deposit(&envelope(1, 1, 40), NOW).unwrap().unwrap(), a);

    let found = store.fetch(&[[1; 32]], NOW).unwrap();
    // Same `stored_at`, so the store's `ORDER BY stored_at, id` returns them by id.
    let mut expected = vec![a, b];
    expected.sort_unstable();
    assert_eq!(found.iter().map(|e| e.id).collect::<Vec<_>>(), expected);
    assert_eq!(store.delete(&[a, b, [9; 32]]).unwrap(), 2);
    assert_eq!(store.fetch(&[[1; 32]], NOW).unwrap(), vec![]);
    assert_eq!(store.usage().unwrap().envelopes, 1);
}

#[test]
fn deposits_outside_the_limits_are_refused() {
    let limits = Limits {
        max_envelopes_per_mailbox: 2,
        max_total_bytes: 200,
        ..Limits::default()
    };
    let store = MailboxStore::in_memory(limits).unwrap();
    assert_eq!(
        store
            .deposit(&envelope(1, 1, MAX_ENVELOPE_BYTES + 1), NOW)
            .unwrap(),
        Err(Refusal::TooLarge)
    );
    let mut stale = envelope(1, 1, 10);
    stale.expires_at = NOW;
    assert_eq!(store.deposit(&stale, NOW).unwrap(), Err(Refusal::BadExpiry));
    let mut far = envelope(1, 1, 10);
    far.expires_at = NOW + limits.max_ttl_secs + 1;
    assert_eq!(store.deposit(&far, NOW).unwrap(), Err(Refusal::BadExpiry));

    store.deposit(&envelope(1, 1, 10), NOW).unwrap().unwrap();
    store.deposit(&envelope(1, 2, 10), NOW).unwrap().unwrap();
    assert_eq!(
        store.deposit(&envelope(1, 3, 10), NOW).unwrap(),
        Err(Refusal::QuotaExceeded)
    );
    store.deposit(&envelope(2, 4, 150), NOW).unwrap().unwrap();
    assert_eq!(
        store.deposit(&envelope(3, 5, 50), NOW).unwrap(),
        Err(Refusal::QuotaExceeded)
    );
}

#[test]
fn expired_envelopes_are_neither_served_nor_kept() {
    let store = MailboxStore::in_memory(Limits::default()).unwrap();
    store.deposit(&envelope(1, 1, 10), NOW).unwrap().unwrap();
    assert_eq!(store.fetch(&[[1; 32]], NOW + 3600).unwrap(), vec![]);
    assert_eq!(store.purge(NOW + 3600).unwrap(), 1);
    assert_eq!(store.usage().unwrap().envelopes, 0);
}

#[test]
fn the_database_records_no_depositor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mailbox.db");
    let store = MailboxStore::open(&path, Limits::default()).unwrap();
    store.deposit(&envelope(1, 1, 10), NOW).unwrap().unwrap();
    drop(store);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let mut statement = conn
        .prepare("SELECT name FROM pragma_table_info('envelopes')")
        .unwrap();
    let columns: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        columns,
        ["id", "mailbox", "expires_at", "sealed", "size", "stored_at"]
    );
    let tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 1);
}

#[test]
fn a_fetch_answer_always_fits_the_wire() {
    let store = MailboxStore::in_memory(Limits {
        max_envelopes_per_mailbox: 2000,
        ..Limits::default()
    })
    .unwrap();
    for i in 0..=MAX_FETCH_ENVELOPES {
        let mut tiny = envelope(1, 1, 1);
        tiny.sealed = u32::try_from(i).unwrap().to_be_bytes().to_vec();
        store.deposit(&tiny, NOW).unwrap().unwrap();
    }

    let found = store.fetch(&[[1; 32]], NOW).unwrap();

    assert_eq!(found.len(), MAX_FETCH_ENVELOPES);
}
