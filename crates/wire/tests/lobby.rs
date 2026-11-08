//! Lobby founding: invites have exactly one string form and keep their secret out of logs, and a plan's protocol
//! version must be one every member speaks.

mod support;

use ome_wire::{
    Error, Message, encode_body,
    limits::MAX_INVITE_CHARS,
    messages::lobby::{INVITE_PREFIX, Invite},
    version::VersionRange,
};
use support::{invite, lobby_plan};

#[test]
fn an_invite_round_trips_through_its_string_form() {
    let text = invite().encode().unwrap();

    assert!(text.starts_with(INVITE_PREFIX));
    assert_eq!(Invite::decode(&format!("  {text}\n")).unwrap(), invite());
}

#[test]
fn an_invite_string_has_exactly_one_form() {
    // Pick a network name whose encoding ends in a base64 character with unused low bits (2 characters for the last
    // byte, so 4 unused bits), then set one of them: a lenient decoder would read the same bytes.
    let text = (0..3)
        .map(|n| {
            Invite {
                network: "n".repeat(n),
                ..invite()
            }
            .encode()
            .unwrap()
        })
        .find(|text| (text.len() - INVITE_PREFIX.len()) % 4 == 2)
        .unwrap();
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut lenient = text.clone();
    let last = alphabet.find(lenient.pop().unwrap()).unwrap();
    lenient.push(alphabet.as_bytes()[last + 1].into());

    assert!(Invite::decode(&text).is_ok());
    assert_eq!(Invite::decode(&lenient), Err(Error::Malformed));
    assert_eq!(
        Invite::decode(&text.replace(INVITE_PREFIX, "clave-invite-v0:")),
        Err(Error::Malformed)
    );
    assert!(matches!(
        Invite::decode(&"A".repeat(MAX_INVITE_CHARS + 1)),
        Err(Error::TooLarge(_))
    ));
}

#[test]
fn an_invite_with_impossible_thresholds_is_refused() {
    let mut low = invite();
    low.viewing_threshold = 1;
    let mut high = invite();
    high.threshold = 9;

    assert_eq!(
        low.encode(),
        Err(Error::OutOfBounds {
            field: "viewing_threshold"
        })
    );
    assert_eq!(
        high.encode(),
        Err(Error::OutOfBounds { field: "threshold" })
    );
}

#[test]
fn an_invite_never_prints_its_secret() {
    let printed = format!("{:?}", invite());

    assert!(!printed.contains("50, 50, 50"), "{printed}");
    assert!(!printed.contains("secret"), "{printed}");
}

#[test]
fn a_plan_needs_a_version_every_member_speaks() {
    let mut plan = lobby_plan();
    plan.members[1].keeper.versions = VersionRange { min: 1, max: 2 };

    assert_eq!(
        encode_body(&Message::LobbyPlan(plan)),
        Err(Error::OutOfBounds {
            field: "protocol_version"
        })
    );
}
