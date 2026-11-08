//! Mailboxes: an envelope deposited for an offline member is found once, from every relay that holds it, and deleted
//! everywhere; the in-process mailbox keeps the same contract.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_net::{Libp2pTransport, Mailbox, MemoryMailbox, RelayMailbox};
use ome_wire::messages::mailbox::RelayEnvelope;
use support::{config, keeper, relay};

fn envelope(recipient: u8) -> RelayEnvelope {
    RelayEnvelope {
        recipient: [recipient; 32],
        expires_at: 1_700_086_400,
        sealed: vec![recipient; 40],
    }
}

/// The contract every mailbox keeps.
async fn deposit_fetch_delete(mailbox: &dyn Mailbox) {
    mailbox.deposit(envelope(1)).await.unwrap();
    mailbox.deposit(envelope(2)).await.unwrap();

    let found = mailbox.fetch(vec![[1; 32]]).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].envelope, envelope(1));

    mailbox.delete(vec![found[0].id]).await.unwrap();
    assert_eq!(mailbox.fetch(vec![[1; 32]]).await.unwrap(), vec![]);
    assert_eq!(mailbox.fetch(vec![[2; 32]]).await.unwrap().len(), 1);
}

#[tokio::test]
async fn relays_store_merge_and_delete() {
    let (first, first_store) = relay::spawn(8).await;
    let (second, second_store) = relay::spawn(9).await;
    let (node, _) = Libp2pTransport::start(
        &keeper(1),
        config(&["/ip4/127.0.0.1/tcp/0"], vec![first, second]),
    )
    .await
    .unwrap();
    let mailbox = RelayMailbox::new(node.clone());

    deposit_fetch_delete(&mailbox).await;

    // Both relays held every deposit; the deleted one is gone from both.
    assert_eq!(first_store.lock().unwrap().len(), 1);
    assert_eq!(second_store.lock().unwrap().len(), 1);
    node.stop();
}

#[tokio::test]
async fn the_memory_mailbox_keeps_the_same_contract() {
    let mailbox = MemoryMailbox::default();

    deposit_fetch_delete(&mailbox).await;

    assert_eq!(mailbox.len(), 1);
}
