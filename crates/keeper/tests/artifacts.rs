//! Contract artifacts between keepers: one put by its daemon in chunks completes with a manifest of their hashes; a
//! member's keeper fetches it from the vault's members, chunk by chunk, and reassembles exactly the same bytes.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_api::{Hex32, keeper::KeeperEvent};
use support::{fast_config, vault, wait_for};

/// An artifact of `chunks` chunks (the last one shorter), as a daemon would split it.
fn artifact(chunks: usize) -> Vec<Vec<u8>> {
    (0..chunks)
        .map(|i| {
            let len = if i + 1 == chunks { 1000 } else { 256 * 1024 };
            (0..len)
                .map(|b| u8::try_from((b + i) % 251).unwrap())
                .collect()
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_artifact_put_by_one_member_is_fetched_by_another_in_chunks() {
    let v = vault(fast_config()).await;
    let class_id = [0x5c; 32];
    // More chunks than one request asks for, so the fetch runs in batches.
    let chunks = artifact(20);
    let count = u32::try_from(chunks.len()).unwrap();
    let owner = &v.keepers[0].keeper;
    for (index, bytes) in chunks.iter().enumerate().rev() {
        let progress = owner
            .put_artifact(class_id, u32::try_from(index).unwrap(), count, bytes)
            .unwrap();
        assert_eq!(progress.complete, index == 0);
    }

    let fetcher = &v.keepers[1].keeper;
    let mut events = fetcher.subscribe();
    assert!(
        !fetcher
            .fetch_artifact(v.address, class_id)
            .await
            .unwrap()
            .complete
    );
    let complete = wait_for(&mut events, |e| match e {
        KeeperEvent::Artifact {
            class_id: c,
            complete,
            ..
        } if *c == Hex32(class_id) => Some(*complete),
        _ => None,
    })
    .await;
    assert!(complete);
    for (index, bytes) in chunks.iter().enumerate() {
        let (got_count, got) = fetcher
            .artifact_chunk(class_id, u32::try_from(index).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!((got_count, &got), (count, bytes));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chunk_out_of_bounds_or_a_changed_count_is_refused() {
    let v = vault(fast_config()).await;
    let keeper = &v.keepers[0].keeper;
    let class_id = [0x6d; 32];
    assert!(keeper.put_artifact(class_id, 2, 2, &[1]).is_err());
    assert!(keeper.put_artifact(class_id, 0, 129, &[1]).is_err());
    assert!(
        keeper
            .put_artifact(class_id, 0, 1, &vec![0; 256 * 1024 + 1])
            .is_err()
    );
    keeper.put_artifact(class_id, 0, 2, &[1]).unwrap();
    assert!(keeper.put_artifact(class_id, 1, 3, &[2]).is_err());
    // Nothing is served before the artifact is complete.
    assert_eq!(keeper.artifact_chunk(class_id, 0).unwrap(), None);
}
