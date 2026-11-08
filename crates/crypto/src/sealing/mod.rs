//! Encryption between keepers and devices: authenticated envelopes for ceremony shares, sealed payloads for relay
//! mailboxes, and the pairing mailbox of a device pairing offer.

pub mod envelope;
pub mod mailbox;
pub mod pairing;
