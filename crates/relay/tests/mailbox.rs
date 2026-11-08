//! Keepers' relay mailboxes against real relays: an envelope is deposited at every relay, fetched once, deleted
//! everywhere, and held exactly as sealed; a private relay refuses strangers' deposits.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_net::{Error, Libp2pTransport, Mailbox, RelayMailbox};
use ome_wire::messages::mailbox::RelayEnvelope;
use support::{identity, node, peer_id, relay};

fn envelope(recipient: u8) -> RelayEnvelope {
    RelayEnvelope {
        recipient: [recipient; 32],
        expires_at: support::in_an_hour(),
        sealed: vec![recipient; 40],
    }
}

#[tokio::test]
async fn relays_store_merge_and_delete() {
    let (mut first, first_address) = relay(8, None).await;
    let (mut second, second_address) = relay(9, None).await;
    let (keeper, _) = Libp2pTransport::start(
        &identity(1),
        node(
            &["/ip4/127.0.0.1/tcp/0"],
            vec![first_address, second_address],
        ),
    )
    .await
    .unwrap();
    let mailbox = RelayMailbox::new(keeper.clone());

    mailbox.deposit(envelope(1)).await.unwrap();
    mailbox.deposit(envelope(2)).await.unwrap();

    let found = mailbox.fetch(vec![[1; 32]]).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].envelope, envelope(1));
    // Each relay holds both envelopes, exactly as sealed.
    for relay in [&first, &second] {
        assert_eq!(
            relay
                .store
                .fetch(&[[1; 32]], support::in_an_hour() - 3600)
                .unwrap()[0]
                .envelope
                .sealed,
            envelope(1).sealed
        );
        assert_eq!(relay.store.usage().unwrap().envelopes, 2);
    }

    mailbox.delete(vec![found[0].id]).await.unwrap();
    assert_eq!(mailbox.fetch(vec![[1; 32]]).await.unwrap(), vec![]);
    assert_eq!(first.store.usage().unwrap().envelopes, 1);
    assert_eq!(second.store.usage().unwrap().envelopes, 1);
    keeper.stop();
    first.stop();
    second.stop();
}

#[tokio::test]
async fn a_private_relay_refuses_a_strangers_deposit() {
    let (mut relay, address) = relay(9, Some([peer_id(1)].into())).await;
    let (stranger, _) =
        Libp2pTransport::start(&identity(2), node(&["/ip4/127.0.0.1/tcp/0"], vec![address]))
            .await
            .unwrap();

    let refused = RelayMailbox::new(stranger.clone())
        .deposit(envelope(1))
        .await;

    assert_eq!(refused, Err(Error::NotStored));
    assert_eq!(relay.store.usage().unwrap().envelopes, 0);
    stranger.stop();
    relay.stop();
}
