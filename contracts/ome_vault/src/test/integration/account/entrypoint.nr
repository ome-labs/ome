//! The account entrypoint: members approve a payload's hash, the vault checks k approvals, then handles fees and
//! cancellation exactly as Aztec's default account entrypoint does.
//!
//! Paying from pre-existing fee juice ends the setup phase, which the TXE cannot do inside an inline private context;
//! that path is left to the local-network end-to-end suite.

use crate::test::fixtures::{
    account::{empty_payload, entrypoint_hash, run_entrypoint},
    approvals::first,
    vault::{ONE_DAY, TestVault},
};
use aztec::authwit::{
    account::{AccountFeePaymentMethodOptions, compute_tx_nullifier},
    entrypoint::app::AppPayload,
};

global TX_NONCE: Field = 0x5eed;

/// A witness in which `approvers` approve running `app` through the vault's entrypoint.
unconstrained fn approve_payload(vault: TestVault, app: AppPayload, approvers: u32) -> [Field; 49] {
    let expires_at = vault.now() + ONE_DAY;
    let message = vault.approval_message(entrypoint_hash(vault, app), 0, expires_at);
    vault.witness(message, first(approvers), expires_at)
}

#[test]
unconstrained fn runs_a_payload_approved_by_k_members() {
    let vault = TestVault::found(3, 2, 2);
    let app = empty_payload(TX_NONCE);

    let _ = run_entrypoint(
        vault,
        app,
        AccountFeePaymentMethodOptions.EXTERNAL,
        false,
        approve_payload(vault, app, 2),
    );
}

#[test(should_fail)]
unconstrained fn refuses_a_payload_with_too_few_approvals() {
    let vault = TestVault::found(3, 2, 2);
    let app = empty_payload(TX_NONCE);

    let _ = run_entrypoint(
        vault,
        app,
        AccountFeePaymentMethodOptions.EXTERNAL,
        false,
        approve_payload(vault, app, 1),
    );
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn refuses_approvals_of_another_payload() {
    let vault = TestVault::found(3, 2, 2);
    let approved = empty_payload(TX_NONCE);

    let _ = run_entrypoint(
        vault,
        empty_payload(TX_NONCE + 1),
        AccountFeePaymentMethodOptions.EXTERNAL,
        false,
        approve_payload(vault, approved, 2),
    );
}

#[test]
unconstrained fn a_cancellable_run_emits_the_nonce_nullifier() {
    let vault = TestVault::found(3, 2, 2);
    let app = empty_payload(TX_NONCE);

    let effects = run_entrypoint(
        vault,
        app,
        AccountFeePaymentMethodOptions.EXTERNAL,
        true,
        approve_payload(vault, app, 2),
    );

    assert_eq(effects.nullifier_count, 1);
    assert_eq(effects.first_nullifier, compute_tx_nullifier(TX_NONCE));
}

#[test]
unconstrained fn a_non_cancellable_run_emits_no_nullifier() {
    let vault = TestVault::found(3, 2, 2);
    let app = empty_payload(TX_NONCE);

    let effects = run_entrypoint(
        vault,
        app,
        AccountFeePaymentMethodOptions.EXTERNAL,
        false,
        approve_payload(vault, app, 2),
    );

    assert_eq(effects.nullifier_count, 0);
}

#[test]
unconstrained fn an_external_fee_payer_leaves_the_vault_out_of_fees() {
    let vault = TestVault::found(3, 2, 2);
    let app = empty_payload(TX_NONCE);

    let effects = run_entrypoint(
        vault,
        app,
        AccountFeePaymentMethodOptions.EXTERNAL,
        false,
        approve_payload(vault, app, 2),
    );

    assert(!effects.is_fee_payer);
}

#[test]
unconstrained fn paying_with_a_fee_juice_claim_makes_the_vault_the_fee_payer() {
    let vault = TestVault::found(3, 2, 2);
    let app = empty_payload(TX_NONCE);

    let effects = run_entrypoint(
        vault,
        app,
        AccountFeePaymentMethodOptions.FEE_JUICE_WITH_CLAIM,
        false,
        approve_payload(vault, app, 2),
    );

    assert(effects.is_fee_payer);
}
