//! Replay protection: one delivery per sender and id, inside the epoch and the time window.

mod support;

use ome_wire::{Error, Header, ReplayGuard};
use support::{NOW, header, identity};

const SKEW: u64 = 300;

fn at(id: u8, created_at: u64) -> Header {
    Header {
        created_at,
        ..header(id)
    }
}

#[test]
fn a_message_is_accepted_once_per_sender() {
    let mut guard = ReplayGuard::new(SKEW, 16);
    let (alice, bob) = (identity(1).public(), identity(2).public());

    assert_eq!(guard.check(&alice, &header(1), 3, NOW), Ok(()));
    assert_eq!(
        guard.check(&alice, &header(1), 3, NOW),
        Err(Error::Replayed)
    );
    assert_eq!(guard.check(&bob, &header(1), 3, NOW), Ok(()));
}

#[test]
fn messages_arrive_out_of_order_inside_the_window() {
    let mut guard = ReplayGuard::new(SKEW, 16);
    let alice = identity(1).public();

    assert_eq!(guard.check(&alice, &at(2, NOW + 10), 3, NOW), Ok(()));
    assert_eq!(guard.check(&alice, &at(1, NOW - 10), 3, NOW), Ok(()));
}

#[test]
fn stale_future_and_other_epoch_messages_are_rejected() {
    let mut guard = ReplayGuard::new(SKEW, 16);
    let alice = identity(1).public();

    assert_eq!(
        guard.check(&alice, &at(1, NOW - SKEW - 1), 3, NOW),
        Err(Error::Stale)
    );
    assert_eq!(
        guard.check(&alice, &at(2, NOW + SKEW + 1), 3, NOW),
        Err(Error::Stale)
    );
    assert_eq!(
        guard.check(&alice, &header(3), 4, NOW),
        Err(Error::WrongEpoch {
            expected: 4,
            actual: 3
        })
    );
}

#[test]
fn an_evicted_id_cannot_be_replayed_once_it_leaves_the_window() {
    let mut guard = ReplayGuard::new(SKEW, 2);
    let alice = identity(1).public();
    guard.check(&alice, &header(1), 3, NOW).unwrap();
    guard.check(&alice, &header(2), 3, NOW).unwrap();
    guard.check(&alice, &header(3), 3, NOW).unwrap();

    // Id 1 was evicted, but by the time the window has moved past it, its timestamp is stale.
    assert_eq!(
        guard.check(&alice, &header(1), 3, NOW + SKEW + 1),
        Err(Error::Stale)
    );
}
