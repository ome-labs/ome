//! Pinned encodings of every message kind, the invite string and the mailbox messages. Any difference is a wire-format
//! change: append new variants or add a protocol version. The TypeScript and WebAssembly encoders check against the
//! same file. Regenerate with `OME_UPDATE_VECTORS=1 cargo test -p ome-wire --test vectors` and review the diff.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use std::collections::BTreeMap;

use ome_wire::{
    encode,
    messages::mailbox::{
        MailboxRequest, MailboxResponse, StoredEnvelope, encode_request, encode_response,
    },
    version::PROTOCOL_VERSION,
};
use serde_json::{Value, json};
use support::{invite, pair_offer, relay_envelope, samples, signed};

const VECTORS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors/wire.json");

fn current() -> Value {
    let messages: BTreeMap<&str, String> = samples()
        .into_iter()
        .map(|(name, body)| (name, hex::encode(encode(&signed(body)).unwrap())))
        .collect();
    let envelope = relay_envelope();
    let mailbox = BTreeMap::from([
        (
            "deposit",
            hex::encode(encode_request(&MailboxRequest::Deposit(envelope.clone())).unwrap()),
        ),
        (
            "envelopes",
            hex::encode(
                encode_response(&MailboxResponse::Envelopes(vec![StoredEnvelope {
                    id: envelope.id().unwrap(),
                    envelope,
                }]))
                .unwrap(),
            ),
        ),
    ]);
    json!({
        "source": "ome-wire tests/vectors.rs",
        "protocolVersion": PROTOCOL_VERSION,
        "messages": messages,
        "invite": invite().encode().unwrap(),
        "pairOffer": pair_offer().encode().unwrap(),
        "mailbox": mailbox,
    })
}

#[test]
fn encodings_match_the_pinned_vectors() {
    let actual = current();
    if std::env::var_os("OME_UPDATE_VECTORS").is_some() {
        std::fs::write(
            VECTORS,
            serde_json::to_string_pretty(&actual).unwrap() + "\n",
        )
        .unwrap();
    }
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(VECTORS).unwrap()).unwrap();

    assert_eq!(actual, expected);
}
