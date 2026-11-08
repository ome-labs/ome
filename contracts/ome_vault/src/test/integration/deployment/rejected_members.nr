//! The constructor refuses a malformed signer set; a vault that was never valid is never created.

use crate::{constants::MAX_SIGNERS, point::Point};
use crate::test::fixtures::{
    members::Members,
    points::{distinct_points, FOUNDING_SHARES, off_curve_point, point_from_scalar},
    vault::deploy_vault,
};
use aztec::test::helpers::test_environment::TestEnvironment;

/// Deploys a 2-of-`count` vault (t_v = 2) over `keys`.
unconstrained fn found_over(mut env: TestEnvironment, keys: [Point; MAX_SIGNERS], count: u32) {
    let _ = deploy_vault(
        &mut env,
        keys,
        count,
        2,
        2,
        distinct_points(count, FOUNDING_SHARES),
    );
}

#[test(should_fail_with = "signer count out of range")]
unconstrained fn rejects_a_vault_without_members() {
    let _ = deploy_vault(
        &mut TestEnvironment::new(),
        [Point::empty(); MAX_SIGNERS],
        0,
        0,
        2,
        [Point::empty(); MAX_SIGNERS],
    );
}

#[test(should_fail_with = "signer count out of range")]
unconstrained fn rejects_more_than_eight_members() {
    let mut env = TestEnvironment::new();
    let members = Members::create(&mut env, MAX_SIGNERS);

    let _ = deploy_vault(
        &mut env,
        members.keys,
        MAX_SIGNERS + 1,
        1,
        2,
        distinct_points(MAX_SIGNERS, FOUNDING_SHARES),
    );
}

#[test(should_fail_with = "duplicate signer")]
unconstrained fn rejects_the_same_key_twice() {
    let mut env = TestEnvironment::new();
    let members = Members::create(&mut env, 3);

    let mut keys = members.keys;
    keys[2] = keys[0];

    found_over(env, keys, 3);
}

#[test(should_fail_with = "signer key not on curve")]
unconstrained fn rejects_a_key_off_the_curve() {
    let mut env = TestEnvironment::new();
    let members = Members::create(&mut env, 3);

    let mut keys = members.keys;
    keys[1] = off_curve_point();

    found_over(env, keys, 3);
}

#[test(should_fail_with = "empty signer key")]
unconstrained fn rejects_a_missing_key_inside_the_member_count() {
    let mut env = TestEnvironment::new();
    let members = Members::create(&mut env, 2);

    found_over(env, members.keys, 3);
}

#[test(should_fail_with = "non-empty signer slot past count")]
unconstrained fn rejects_a_key_past_the_member_count() {
    let mut env = TestEnvironment::new();
    let members = Members::create(&mut env, 3);

    found_over(env, members.keys, 2);
}

#[test(should_fail_with = "non-empty signer slot past count")]
unconstrained fn rejects_a_stray_key_in_the_last_slot() {
    let mut env = TestEnvironment::new();
    let members = Members::create(&mut env, 2);

    let mut keys = members.keys;
    keys[MAX_SIGNERS - 1] = point_from_scalar(42);

    found_over(env, keys, 2);
}
