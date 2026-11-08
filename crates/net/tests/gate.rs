//! Admission: members connect and use every lane; strangers only reach an open lobby's lane, within its rate; removal
//! and closing the lobby take effect at once.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use std::time::{Duration, Instant};

use ome_net::{Admission, LOBBY_REQUESTS_PER_MINUTE, framing::Lane};
use support::keeper;

#[test]
fn members_and_relays_are_admitted_and_strangers_are_not() {
    let (member, relay, stranger) = (keeper(1).public(), keeper(2).public(), keeper(3).public());
    let mut admission = Admission::default();
    admission.set([member], false);
    admission.add_relay(relay);

    assert!(admission.admits(&member));
    assert!(admission.admits(&relay));
    assert!(!admission.admits(&stranger));
    assert!(admission.accepts(&member, Lane::Ceremony, Instant::now()));
    assert!(!admission.accepts(&stranger, Lane::Lobby, Instant::now()));
}

#[test]
fn removal_from_the_manifest_closes_admission() {
    let member = keeper(1).public();
    let mut admission = Admission::default();
    admission.set([member], false);

    admission.set([], false);

    assert!(!admission.admits(&member));
    assert!(!admission.accepts(&member, Lane::Sync, Instant::now()));
}

#[test]
fn an_open_lobby_admits_strangers_only_on_its_lane() {
    let stranger = keeper(3).public();
    let mut admission = Admission::default();
    admission.set([], true);
    let now = Instant::now();

    assert!(admission.admits(&stranger));
    assert!(!admission.is_known(&stranger));
    assert!(admission.accepts(&stranger, Lane::Lobby, now));
    for lane in [Lane::Ecdh, Lane::Ceremony, Lane::Sync] {
        assert!(!admission.accepts(&stranger, lane, now), "{lane:?}");
    }

    admission.set([], false);
    assert!(!admission.admits(&stranger));
}

#[test]
fn a_stranger_gets_ten_lobby_requests_a_minute() {
    let (stranger, other) = (keeper(3).public(), keeper(4).public());
    let mut admission = Admission::default();
    admission.set([], true);
    let start = Instant::now();

    for _ in 0..LOBBY_REQUESTS_PER_MINUTE {
        assert!(admission.accepts(&stranger, Lane::Lobby, start));
    }
    assert!(!admission.accepts(&stranger, Lane::Lobby, start));
    assert!(admission.accepts(&other, Lane::Lobby, start));
    assert!(admission.accepts(&stranger, Lane::Lobby, start + Duration::from_secs(60)));
}
