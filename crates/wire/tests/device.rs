//! Pairing offers: the string form round-trips, a tampered or relay-less offer is refused, and `Debug` never shows
//! the secret.
#![allow(clippy::unwrap_used)]

mod support;

use ome_wire::{Error, messages::device::PairOffer};
use support::pair_offer;

#[test]
fn an_offer_round_trips_through_its_string_form() {
    let offer = pair_offer();
    let text = offer.encode().unwrap();
    assert!(text.starts_with("ome-pair-v0:"));
    assert_eq!(PairOffer::decode(&format!("  {text}\n")).unwrap(), offer);
}

#[test]
fn an_offer_without_a_relay_or_with_another_prefix_is_refused() {
    let mut offer = pair_offer();
    offer.relays.clear();
    assert!(matches!(
        offer.encode(),
        Err(Error::OutOfBounds { field: "relays" })
    ));
    let text = pair_offer().encode().unwrap();
    let other = text.replacen("ome-pair-v0:", "ome-invite-v0:", 1);
    assert_eq!(PairOffer::decode(&other), Err(Error::Malformed));
}

#[test]
fn debug_omits_the_secret() {
    let shown = format!("{:?}", pair_offer());
    assert!(!shown.contains("secret"));
    assert!(!shown.contains("89, 89"));
}
