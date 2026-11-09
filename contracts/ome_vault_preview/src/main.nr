use aztec::macros::aztec;

// Its functions keep the vault's selectors.
mod test;

/// Simulation stand-in for `OmeVault`, never deployed. A member's wallet previews a proposal before anyone has signed
/// it by overriding the vault's class with this one for that simulation only: the same storage and the same functions
/// a proposal can call on the vault, built from `ome_vault`'s own modules, with an entrypoint that accepts the
/// transaction without approvals. A proposal that calls the vault itself (`rotate_membership`, `set_threshold`) then
/// runs the vault's real checks, so one the contract would refuse is caught at review.
///
/// Keep the storage and these functions' signatures identical to `OmeVault` (its `preview` test checks the selectors).
#[aztec]
pub contract OmeVaultPreview {
    use aztec::{
        authwit::{account::AccountActions, entrypoint::app::AppPayload},
        context::PrivateContext,
        keys::getters::get_public_keys,
        macros::{functions::{allow_phase_change, external, only_self, view}, storage::storage},
        messages::delivery::{MessageDelivery, OnchainConstrainedDelivery},
        note::note_getter_options::NoteGetterOptions,
        protocol::address::AztecAddress,
        state_vars::{Owned, PrivateSet, SinglePrivateMutable},
    };
    use ome_vault::{
        constants::MAX_SIGNERS,
        governance,
        notes::{config_note::ConfigNote, signer_note::SignerNote},
        point::Point,
        signer_set::SignerSet,
    };

    #[storage]
    struct Storage<Context> {
        config: SinglePrivateMutable<ConfigNote, Context>,
        signers: Owned<PrivateSet<SignerNote, Context>, Context>,
    }

    #[external("private")]
    #[only_self]
    fn rotate_membership(
        current_signer_keys: [Point; MAX_SIGNERS],
        new_signer_keys: [Point; MAX_SIGNERS],
        new_signer_count: u32,
        threshold: u32,
        viewing_threshold: u32,
        committee_shares: [Point; MAX_SIGNERS],
        committee_polynomial: [Point; MAX_SIGNERS],
    ) {
        let ivpk = get_public_keys(self.address).ivpk_m.inner;
        let viewing_key = Point { x: ivpk.x, y: ivpk.y };
        self
            .storage
            .config
            .replace(
                |config| governance::rotate(
                    config,
                    current_signer_keys,
                    new_signer_keys,
                    new_signer_count,
                    threshold,
                    viewing_threshold,
                    committee_shares,
                    committee_polynomial,
                    viewing_key,
                ),
                self.address,
            )
            .deliver(config_delivery(self.address));

        let signer_notes = self.storage.signers.at(self.address);
        let _ = signer_notes.pop_notes(NoteGetterOptions::new());
        let signers = SignerSet { keys: new_signer_keys, count: new_signer_count };
        let delivery = MessageDelivery::onchain_unconstrained().with_sender(self.address);
        for slot in 0..MAX_SIGNERS {
            if slot < signers.count {
                let key = signers.keys[slot];
                signer_notes.insert(SignerNote { x: key.x, y: key.y, slot }).deliver(delivery);
            }
        }
    }

    #[external("private")]
    #[allow_phase_change]
    fn entrypoint(app_payload: AppPayload, fee_payment_method: u8, cancellable: bool) {
        let actions = AccountActions::init(self.context, accept);
        actions.entrypoint(app_payload, fee_payment_method, cancellable);
    }

    #[external("private")]
    #[view]
    fn verify_private_authwit(inner_hash: Field) -> Field {
        let actions = AccountActions::init(self.context, accept);
        actions.verify_private_authwit(inner_hash)
    }

    #[external("private")]
    #[only_self]
    fn set_threshold(threshold: u32) {
        self
            .storage
            .config
            .replace(|config| governance::with_threshold(config, threshold), self.address)
            .deliver(config_delivery(self.address));
    }

    #[external("private")]
    #[only_self]
    fn invalidate_pending() {
        self.storage.config.replace(governance::bump_version, self.address).deliver(config_delivery(
            self.address,
        ));
    }

    /// Accepts every transaction: the preview runs before members sign.
    #[contract_library_method]
    fn accept(_context: &mut PrivateContext, _outer_hash: Field) -> bool {
        true
    }

    #[contract_library_method]
    fn config_delivery(vault: AztecAddress) -> OnchainConstrainedDelivery {
        MessageDelivery::onchain_constrained().with_sender(vault)
    }
}
