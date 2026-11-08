//! Pins the protocol constants. They are a wire format shared with the Rust and TypeScript encoders, so a change here
//! must be deliberate: update every encoder and the cross-language vectors together.

use crate::constants::{
    CONFIG_MAX_AGE, DOM_SEP__APPROVAL, DOM_SEP__COMMITTEE, DOM_SEP__SIGNER_SET, MAX_SIGNERS,
    MIN_VIEWING_THRESHOLD, POINT_LEN, SIGNATURE_LEN, WITNESS_LEN,
};

/// The field whose big-endian bytes spell `text`, as the domain separators are written.
fn ascii<let N: u32>(text: str<N>) -> Field {
    let bytes = text.as_bytes();
    let mut field = 0;
    for i in 0..N {
        field = field * 256 + bytes[i] as Field;
    }
    field
}

#[test]
fn domain_separators_spell_their_names() {
    assert_eq(DOM_SEP__APPROVAL, ascii("ome.approval.v0"));
    assert_eq(DOM_SEP__SIGNER_SET, ascii("ome.signers.v0"));
    assert_eq(DOM_SEP__COMMITTEE, ascii("ome.committee.v0"));
}

#[test]
fn domain_separators_are_distinct() {
    assert(DOM_SEP__APPROVAL != DOM_SEP__SIGNER_SET);
    assert(DOM_SEP__APPROVAL != DOM_SEP__COMMITTEE);
    assert(DOM_SEP__SIGNER_SET != DOM_SEP__COMMITTEE);
}

#[test]
fn witness_holds_eight_keys_eight_signatures_and_a_deadline() {
    assert_eq(MAX_SIGNERS, 8);
    assert_eq(POINT_LEN, 2);
    assert_eq(SIGNATURE_LEN, 4);
    assert_eq(WITNESS_LEN, 49);
}

#[test]
fn no_single_member_can_read_a_vault() {
    assert_eq(MIN_VIEWING_THRESHOLD, 2);
}

#[test]
fn a_superseded_config_authorizes_for_at_most_an_hour() {
    assert_eq(CONFIG_MAX_AGE, 3600);
}
