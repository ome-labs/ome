//! The vault's account entry points, run the way the contract runs them.
//!
//! The TXE serves auth witnesses to real contract calls only from its own single-key signer, so a k-of-n witness
//! cannot reach `entrypoint` or `verify_private_authwit` through a real call. These helpers run Aztec's
//! `AccountActions` (the code both entry points delegate to) in a private context at the vault, wired to the vault's
//! own `is_authorized`, while the PXE serves the chosen witness. The TXE also forbids contract calls from such a
//! context, so payloads here carry no calls; governance effects are tested through calls from the vault itself.

use crate::fixtures::vault::TestVault;
use aztec::{
    authwit::{
        account::AccountActions, auth::compute_authwit_message_hash, entrypoint::app::AppPayload,
    },
    context::PrivateContext,
    protocol::{address::AztecAddress, traits::{Deserialize, Hash}},
};
use ome_vault::{authorization::is_authorized, constants::WITNESS_LEN, OmeVault};
use std::test::OracleMock;

/// Fields of a serialized `AppPayload`: five calls of six fields each, then the nonce.
global APP_PAYLOAD_LEN: u32 = 31;

/// What a run of the entrypoint left in its transaction.
pub struct EntrypointEffects {
    pub nullifier_count: u32,
    pub first_nullifier: Field,
    pub is_fee_payer: bool,
}

/// The vault's `is_valid_impl`: authorization against the stored config note.
fn vault_is_valid(context: &mut PrivateContext, outer_hash: Field) -> bool {
    is_authorized(context, OmeVault::storage_layout().config.slot, outer_hash)
}

/// A payload with no calls and the given cancellation nonce.
pub fn empty_payload(tx_nonce: Field) -> AppPayload {
    let mut fields = [0; APP_PAYLOAD_LEN];
    fields[APP_PAYLOAD_LEN - 1] = tx_nonce;
    AppPayload::deserialize(fields)
}

/// The chain id and protocol version the TXE's private contexts run under.
pub unconstrained fn chain(vault: TestVault) -> (Field, Field) {
    vault.env.private_context(|context| (context.chain_id(), context.version()))
}

/// The hash the vault's members approve so that `app` may run through the entrypoint.
pub unconstrained fn entrypoint_hash(vault: TestVault, app: AppPayload) -> Field {
    let (chain_id, version) = chain(vault);
    compute_authwit_message_hash(vault.address, chain_id, version, app.hash())
}

/// The hash the vault's members approve so that `consumer` may act on `inner_hash` for the vault.
pub unconstrained fn authwit_hash(
    vault: TestVault,
    consumer: AztecAddress,
    inner_hash: Field,
) -> Field {
    let (chain_id, version) = chain(vault);
    compute_authwit_message_hash(consumer, chain_id, version, inner_hash)
}

/// Runs the vault's entrypoint over `app` while the PXE serves `witness`.
pub unconstrained fn run_entrypoint(
    vault: TestVault,
    app: AppPayload,
    fee_payment_method: u8,
    cancellable: bool,
    witness: [Field; WITNESS_LEN],
) -> EntrypointEffects {
    let _ = OracleMock::mock("aztec_utl_getAuthWitness").returns(witness).times(1);

    vault.env.private_context_at(vault.address, |context| {
        AccountActions::init(context, vault_is_valid).entrypoint(
            app,
            fee_payment_method,
            cancellable,
        );

        let inputs = context.finish();
        EntrypointEffects {
            nullifier_count: inputs.nullifiers.length,
            first_nullifier: inputs.nullifiers.array[0].inner.value,
            is_fee_payer: inputs.is_fee_payer,
        }
    })
}
