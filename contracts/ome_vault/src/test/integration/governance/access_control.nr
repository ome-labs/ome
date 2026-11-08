//! Policy changes are `#[only_self]`: only an approved vault transaction, executing as the vault, may make them.

use crate::OmeVault;
use crate::test::fixtures::vault::TestVault;

#[test(should_fail_with = "can only be called by the same contract")]
unconstrained fn a_member_cannot_set_the_threshold_directly() {
    let vault = TestVault::found(3, 2, 2);

    vault.env.call_private(vault.members.accounts[0], OmeVault::at(vault.address).set_threshold(1));
}

#[test(should_fail_with = "can only be called by the same contract")]
unconstrained fn a_member_cannot_invalidate_pending_approvals_directly() {
    let vault = TestVault::found(3, 2, 2);

    vault.env.call_private(
        vault.members.accounts[1],
        OmeVault::at(vault.address).invalidate_pending(),
    );
}

#[test(should_fail_with = "can only be called by the same contract")]
unconstrained fn a_member_cannot_rotate_membership_directly() {
    let vault = TestVault::found(3, 2, 2);

    vault.env.call_private(
        vault.members.accounts[0],
        OmeVault::at(vault.address).rotate_membership(
            vault.members.keys,
            vault.members.keys,
            3,
            2,
            2,
            vault.committee_shares,
        ),
    );
}
