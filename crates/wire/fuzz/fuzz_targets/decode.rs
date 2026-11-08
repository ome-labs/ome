//! Arbitrary bytes never panic a decoder, and whatever a decoder accepts re-encodes to the same bytes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ome_wire::{
    decode, decode_body, encode, encode_body,
    messages::{
        lobby::Invite,
        mailbox::{decode_request, decode_response, encode_request, encode_response},
    },
};

fuzz_target!(|bytes: &[u8]| {
    if let Ok(message) = decode(bytes) {
        let _ = message.verify();
        assert_eq!(encode(&message).ok().as_deref(), Some(bytes));
    }
    if let Ok(body) = decode_body(bytes) {
        assert_eq!(encode_body(&body).ok().as_deref(), Some(bytes));
    }
    if let Ok(request) = decode_request(bytes) {
        assert_eq!(encode_request(&request).ok().as_deref(), Some(bytes));
    }
    if let Ok(response) = decode_response(bytes) {
        assert_eq!(encode_response(&response).ok().as_deref(), Some(bytes));
    }
    if let Ok(text) = core::str::from_utf8(bytes)
        && let Ok(invite) = Invite::decode(text)
    {
        assert_eq!(invite.encode().ok().as_deref(), Some(text.trim()));
    }
});
