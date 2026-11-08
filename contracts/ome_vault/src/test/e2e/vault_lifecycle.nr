//! A vault's life: founded 2-of-3, pays, tightens its threshold, takes on a member, loses one, and keeps working with
//! the right people at every step.

use crate::OmeVault;
use crate::test::fixtures::{
    account::{empty_payload, entrypoint_hash, run_entrypoint},
    approvals::{first, slots},
    members::new_member,
    vault::{ONE_DAY, OUTER_HASH, TestVault},
};
use aztec::authwit::account::AccountFeePaymentMethodOptions;

#[test]
unconstrained fn founded_paying_growing_and_shrinking() {
    // Founded by three, any two can pay.
    let mut vault = TestVault::found(3, 2, 2);
    let payment = empty_payload(1);
    let expires_at = vault.now() + ONE_DAY;
    let approval = vault.approval_message(entrypoint_hash(vault, payment), 0, expires_at);

    let _ = run_entrypoint(
        vault,
        payment,
        AccountFeePaymentMethodOptions.EXTERNAL,
        true,
        vault.witness(approval, slots([0, 2]), expires_at),
    );

    // The members tighten the policy to all three.
    vault.self_call(OmeVault::at(vault.address).set_threshold(3));

    assert(!vault.authorize(OUTER_HASH, vault.approve(1, first(2))));
    assert(vault.authorize(OUTER_HASH, vault.approve(1, first(3))));

    // A fourth member joins; three of four now spend and three of four read.
    let joined = vault.members.joined_by(new_member(&mut vault.env));
    let grown = vault.rotate_to(joined, 3, 3);

    assert(grown.authorize(OUTER_HASH, grown.approve(2, slots([1, 2, 3]))));

    // A founder leaves; the remaining three keep the vault and its address.
    let shrunk = grown.rotate_to(grown.members.without(0), 2, 2);
    let config = shrunk.config();

    assert_eq(shrunk.address, vault.address);
    assert_eq(config.version, 3);
    assert_eq(config.signer_count, 3);
    assert(shrunk.authorize(OUTER_HASH, shrunk.approve(3, first(2))));
}
