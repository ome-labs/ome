//! Cross-language vectors. The expected values were computed independently with Aztec 6.0.0's TypeScript Poseidon2 and
//! Grumpkin (`@aztec-labs/foundation`); the Rust and TypeScript encoders must reproduce the same values. A change to a
//! commitment or the approval message breaks these on purpose.

use crate::{authorization::approval_message, constants::MAX_SIGNERS};
use crate::test::fixtures::{
    points::FOUNDING_SHARES,
    policy::{committee, FOUNDING_KEYS, signer_set},
};
use aztec::protocol::{address::AztecAddress, traits::FromField};

#[test]
fn signer_set_commitment_of_three_members() {
    assert_eq(
        signer_set(3, FOUNDING_KEYS).commitment(),
        0x0da54cd640c8d0a93566c8192db160a21e67969233eb60e530aa49f708df2fe0,
    );
}

#[test]
fn signer_set_commitment_of_a_full_vault() {
    assert_eq(
        signer_set(MAX_SIGNERS, FOUNDING_KEYS).commitment(),
        0x2f533b325f98a398d70d1dbe47602fac9082c4a9648a787d616d2291aaf28640,
    );
}

#[test]
fn committee_commitment_of_two_of_three() {
    assert_eq(
        committee(2, 3, FOUNDING_SHARES).commitment(),
        0x2b5bff0f2c8cdb806c0c362207b6fdba2c616a2f521a036c4381bed590de6874,
    );
}

#[test]
fn committee_commitment_of_eight_of_eight() {
    assert_eq(
        committee(MAX_SIGNERS, MAX_SIGNERS, FOUNDING_SHARES).commitment(),
        0x11dca74ff3e643dc64a7e6b7858316fc4658b6ef7a3b9d9dcbe54168b3578dcf,
    );
}

#[test]
fn approval_message_of_a_typical_payment() {
    let message = approval_message(AztecAddress::from_field(0x1234), 0x5678, 3, 1700000000);

    assert_eq(message, 0x26356f6fcdab1ec9919ef753b2377e907504d356805c652891f841a9ca8030be);
}

#[test]
fn approval_message_with_the_largest_version_and_deadline() {
    let message = approval_message(
        AztecAddress::from_field(0x1234),
        0x5678,
        0xffffffff,
        0xffffffffffffffff,
    );

    assert_eq(message, 0x0e3bdfefec6a7d0e46df0941f7fdbd80bc328b17f24136afa200040e833545f3);
}
