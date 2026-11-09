//! The selector and argument layout of `rotate_membership`, as the account entrypoint calls it. Approvers decode the
//! call's arguments to check a rotation before signing (`ome-protocol`'s `RotationCall`), so both sides pin the same
//! selector and field order.

use crate::OmeVault;
use crate::test::fixtures::{
    points::{distinct_points, FOUNDING_SHARES, ROTATED_SHARES},
    policy::FOUNDING_KEYS,
};
use aztec::protocol::{
    abis::function_selector::FunctionSelector, address::AztecAddress, traits::FromField,
};

#[test]
fn rotate_membership_has_the_selector_approvers_expect() {
    let current = distinct_points(3, FOUNDING_KEYS);
    let next = distinct_points(3, FOUNDING_KEYS + 10);
    let shares = distinct_points(3, ROTATED_SHARES);
    let polynomial = distinct_points(2, ROTATED_SHARES + 10);

    let call = OmeVault::at(AztecAddress::from_field(0x1234)).rotate_membership(
        current,
        next,
        3,
        2,
        2,
        shares,
        polynomial,
    );

    assert_eq(call.selector, FunctionSelector::from_u32(0xe1071b7c));
    assert_eq(
        call.selector,
        FunctionSelector::from_signature(
            "rotate_membership([(Field,Field);8],[(Field,Field);8],u32,u32,u32,[(Field,Field);8],[(Field,Field);8])",
        ),
    );
}

#[test]
fn rotate_membership_arguments_are_keys_then_counts_then_shares_then_polynomial() {
    let current = distinct_points(3, FOUNDING_KEYS);
    let next = distinct_points(2, FOUNDING_KEYS + 10);
    let shares = distinct_points(2, FOUNDING_SHARES);
    let polynomial = distinct_points(2, ROTATED_SHARES);

    let args = OmeVault::at(AztecAddress::from_field(0x1234))
        .rotate_membership(current, next, 2, 1, 2, shares, polynomial)
        .args;

    assert_eq(args.len(), 67);
    for i in 0..8 {
        assert_eq(args[2 * i], current[i].x);
        assert_eq(args[2 * i + 1], current[i].y);
        assert_eq(args[16 + 2 * i], next[i].x);
        assert_eq(args[16 + 2 * i + 1], next[i].y);
        assert_eq(args[35 + 2 * i], shares[i].x);
        assert_eq(args[35 + 2 * i + 1], shares[i].y);
        assert_eq(args[51 + 2 * i], polynomial[i].x);
        assert_eq(args[51 + 2 * i + 1], polynomial[i].y);
    }
    assert_eq(args[32], 2);
    assert_eq(args[33], 1);
    assert_eq(args[34], 2);
}
