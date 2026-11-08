//! The relay protocol: requests and answers are bounded both ways, and envelope ids bind the whole envelope.

mod support;

use ome_wire::{
    Error,
    limits::{MAX_ENVELOPE_BYTES, MAX_FETCH_MAILBOXES},
    messages::mailbox::{
        MailboxRequest, MailboxResponse, RelayEnvelope, StoredEnvelope, decode_request,
        decode_response, encode_request, encode_response,
    },
};
use support::relay_envelope;

#[test]
fn requests_and_responses_round_trip() {
    let envelope = relay_envelope();
    let requests = [
        MailboxRequest::Deposit(envelope.clone()),
        MailboxRequest::Fetch {
            mailboxes: vec![[1; 32]],
        },
        MailboxRequest::Delete { ids: vec![[2; 32]] },
    ];
    let responses = [
        MailboxResponse::Stored {
            id: envelope.id().unwrap(),
        },
        MailboxResponse::Envelopes(vec![StoredEnvelope {
            id: envelope.id().unwrap(),
            envelope,
        }]),
        MailboxResponse::Deleted(1),
    ];

    for request in requests {
        assert_eq!(
            decode_request(&encode_request(&request).unwrap()).unwrap(),
            request
        );
    }
    for response in responses {
        assert_eq!(
            decode_response(&encode_response(&response).unwrap()).unwrap(),
            response
        );
    }
}

#[test]
fn oversized_and_empty_requests_are_refused() {
    let mut huge = relay_envelope();
    huge.sealed = vec![0; MAX_ENVELOPE_BYTES + 1];

    assert_eq!(
        encode_request(&MailboxRequest::Deposit(huge.clone())),
        Err(Error::OutOfBounds { field: "sealed" })
    );
    assert_eq!(
        encode_request(&MailboxRequest::Fetch {
            mailboxes: vec![[0; 32]; MAX_FETCH_MAILBOXES + 1]
        }),
        Err(Error::OutOfBounds { field: "mailboxes" })
    );
    assert_eq!(
        encode_request(&MailboxRequest::Delete { ids: vec![] }),
        Err(Error::OutOfBounds { field: "ids" })
    );
    assert_eq!(
        encode_response(&MailboxResponse::Envelopes(vec![StoredEnvelope {
            id: [0; 32],
            envelope: huge
        }])),
        Err(Error::OutOfBounds { field: "sealed" })
    );
}

#[test]
fn an_envelope_id_binds_every_field() {
    let envelope = relay_envelope();
    let changed = [
        RelayEnvelope {
            recipient: [0; 32],
            ..envelope.clone()
        },
        RelayEnvelope {
            expires_at: 0,
            ..envelope.clone()
        },
        RelayEnvelope {
            sealed: vec![0],
            ..envelope.clone()
        },
    ];

    for other in changed {
        assert_ne!(other.id().unwrap(), envelope.id().unwrap());
    }
}
