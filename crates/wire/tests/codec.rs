//! Canonical encoding: every message kind round-trips, and decoding refuses anything that is not exactly one canonical,
//! bounded, supported message.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_wire::{
    Error, Message, decode, decode_body, encode, encode_body,
    limits::{MAX_ECDH_POINTS, MAX_MESSAGE_BYTES, MAX_SUMMARY_BYTES},
    messages::{
        ceremony::Echo,
        ecdh::{EcdhPurpose, EcdhRequest},
        proposals::PayloadRequest,
    },
};
use support::{BODY_TAG, manifest, proposal, samples, signed, wire_point};

#[test]
fn every_message_kind_round_trips_signed_and_unsigned() {
    for (name, body) in samples() {
        let message = signed(body.clone());
        assert_eq!(
            decode(&encode(&message).unwrap()).unwrap(),
            message,
            "{name}"
        );
        assert_eq!(
            decode_body(&encode_body(&body).unwrap()).unwrap(),
            body,
            "{name}"
        );
    }
}

#[test]
fn variant_tags_follow_declaration_order() {
    for (tag, (name, body)) in samples().into_iter().enumerate() {
        let bytes = encode(&signed(body)).unwrap();
        assert_eq!(usize::from(bytes[BODY_TAG]), tag, "{name}");
    }
}

#[test]
fn rejects_trailing_bytes_and_every_truncation() {
    let bytes = encode(&signed(samples().remove(0).1)).unwrap();

    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(decode(&trailing), Err(Error::Malformed));
    for len in 0..bytes.len() {
        assert!(decode(&bytes[..len]).is_err(), "prefix of {len} bytes");
    }
}

#[test]
fn rejects_an_overlong_varint() {
    let bytes = encode(&signed(samples().remove(3).1)).unwrap();
    // The epoch (3) is a one-byte varint at offset 33; `0x83 0x00` is the same value, overlong.
    let mut overlong = bytes[..33].to_vec();
    overlong.extend_from_slice(&[0x83, 0x00]);
    overlong.extend_from_slice(&bytes[34..]);

    assert_eq!(decode(&overlong), Err(Error::Malformed));
}

#[test]
fn rejects_an_unknown_variant() {
    let mut bytes = encode(&signed(samples().remove(3).1)).unwrap();
    bytes[BODY_TAG] = 0x7f;

    assert_eq!(decode(&bytes), Err(Error::Malformed));
}

#[test]
fn rejects_an_unsupported_version() {
    let mut message = signed(samples().remove(3).1);
    message.version = 1;

    assert_eq!(
        decode(&encode(&message).unwrap()),
        Err(Error::UnsupportedVersion(1))
    );
}

#[test]
fn rejects_input_above_the_message_limit() {
    assert!(matches!(
        decode(&vec![0; MAX_MESSAGE_BYTES + 1]),
        Err(Error::TooLarge(_))
    ));
    assert!(matches!(
        decode_body(&vec![0; MAX_MESSAGE_BYTES + 1]),
        Err(Error::TooLarge(_))
    ));
}

#[test]
fn bounds_are_enforced_on_decode() {
    let mut summary = proposal();
    summary.summary = "x".repeat(MAX_SUMMARY_BYTES + 1);
    let mut viewing = manifest();
    viewing.viewing_threshold = 1;
    let cases = [
        (
            Message::EcdhRequest(EcdhRequest {
                purpose: EcdhPurpose::Handshake,
                points: vec![wire_point(2); MAX_ECDH_POINTS + 1],
            }),
            "points",
        ),
        (Message::Echo(Echo { frame: vec![] }), "frame"),
        (Message::Proposal(summary), "summary"),
        (
            Message::PayloadRequest(PayloadRequest { hashes: vec![] }),
            "hashes",
        ),
        (Message::Manifest(viewing), "viewing_threshold"),
    ];

    for (body, field) in cases {
        // `encode` does not validate, so it can build what a dishonest peer would send.
        let mut message = signed(samples().remove(3).1);
        message.body = body.clone();
        assert_eq!(
            decode(&encode(&message).unwrap()),
            Err(Error::OutOfBounds { field })
        );
        assert_eq!(encode_body(&body), Err(Error::OutOfBounds { field }));
    }
}
