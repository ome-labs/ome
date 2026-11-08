pub mod authorization;
pub mod committee;
pub mod constants;
pub mod governance;
pub mod notes;
pub mod point;
pub mod signer_set;
pub mod witness;

// Unit, integration and end-to-end tests. Test functions are compiled out of the OmeVault artifact.
mod test;

use aztec::macros::aztec;

/// Ome vault: private k-of-n account contract.
///
/// Stores the spending policy (signer set, threshold `k`) and the viewing committee commitment (threshold `t_v`,
/// verification shares) in private notes owned by the vault.
#[aztec]
pub contract OmeVault {
    use aztec::{
        authwit::{
            account::AccountActions,
            auth::{compute_authwit_message_hash, compute_authwit_nullifier},
            entrypoint::app::AppPayload,
        },
        context::PrivateContext,
        hash::compute_siloed_nullifier,
        macros::{
            functions::{allow_phase_change, external, initializer, noinitcheck, only_self, view},
            storage::storage,
        },
        messages::delivery::{MessageDelivery, OnchainConstrainedDelivery},
        note::{note_getter_options::NoteGetterOptions, note_viewer_options::NoteViewerOptions},
        oracle::{
            auth_witness::get_auth_witness,
            get_nullifier_membership_witness::get_low_nullifier_membership_witness,
        },
        protocol::address::AztecAddress,
        state_vars::{Owned, PrivateSet, SinglePrivateMutable, StateVariable},
    };

    use crate::{
        authorization,
        committee::Committee,
        constants::{MAX_SIGNERS, WITNESS_LEN},
        governance,
        notes::{config_note::ConfigNote, signer_note::SignerNote},
        point::Point,
        signer_set::SignerSet,
        witness::ApprovalWitness,
    };

    #[storage]
    struct Storage<Context> {
        config: SinglePrivateMutable<ConfigNote, Context>,
        signers: Owned<PrivateSet<SignerNote, Context>, Context>,
    }

    /// Creates the vault for members `0..signer_count`: their signing keys, the spending threshold `k`, the viewing
    /// threshold `t_v` and their viewing verification shares from the distributed key generation.
    #[external("private")]
    #[initializer]
    fn constructor(
        signer_keys: [Point; MAX_SIGNERS],
        signer_count: u32,
        threshold: u32,
        viewing_threshold: u32,
        committee_shares: [Point; MAX_SIGNERS],
    ) {
        let signers = SignerSet::new(signer_keys, signer_count);
        governance::assert_threshold(threshold, signer_count);
        let committee = Committee::new(viewing_threshold, committee_shares, signer_count);

        // Unconstrained delivery: the deployer is a member and every member verifies the delivered config against the
        // agreed manifest before funding the vault, so a bad delivery only bricks an empty vault. Constrained delivery
        // of `MAX_SIGNERS + 1` notes would also exceed the per-call kernel limits.
        //
        // The deployer is usually a separate fee-paying account. Tagging notes with the vault itself as sender lets
        // every member discover them by registering the vault.
        let delivery = MessageDelivery::onchain_unconstrained().with_sender(self.address);

        let config = ConfigNote {
            threshold,
            signer_count,
            version: 0,
            signer_set_commitment: signers.commitment(),
            viewing_threshold,
            committee_commitment: committee.commitment(),
        };
        self.storage.config.initialize(config, self.address).deliver(delivery);

        store_signer_notes(self.storage.signers.at(self.address), signers);
    }

    /// Replaces the signer set, spending threshold and viewing committee in one step. Only callable through an
    /// approved vault transaction.
    ///
    /// `current_signer_keys` must equal the stored signer set; at most `t_v - 1` current members may be absent from the
    /// new set. `committee_shares` are the verification shares produced by the reshare that prepares this rotation:
    /// keepers activate their new shares once this transaction is included.
    #[external("private")]
    #[only_self]
    fn rotate_membership(
        current_signer_keys: [Point; MAX_SIGNERS],
        new_signer_keys: [Point; MAX_SIGNERS],
        new_signer_count: u32,
        threshold: u32,
        viewing_threshold: u32,
        committee_shares: [Point; MAX_SIGNERS],
    ) {
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
                ),
                self.address,
            )
            .deliver(config_delivery(self.address));

        // Live signer notes are exactly the current set, and each can be nullified only once, so popping as many notes
        // as there are current members removes all of them.
        let signer_notes = self.storage.signers.at(self.address);
        let removed = signer_notes.pop_notes(NoteGetterOptions::new());

        // failure-check: invariant. `governance::rotate` already proved `current_signer_keys` match the stored
        // commitment, which fixes how many signer notes are live, so no caller can reach this message.
        assert_eq(
            removed.len(),
            governance::member_count(current_signer_keys),
            "stale signer notes",
        );
        store_signer_notes(
            signer_notes,
            SignerSet { keys: new_signer_keys, count: new_signer_count },
        );
    }

    // @dev: Keep this signature in sync with the default account entrypoint ABI used by Aztec.js.
    #[external("private")]
    #[noinitcheck]
    #[allow_phase_change]
    fn entrypoint(app_payload: AppPayload, fee_payment_method: u8, cancellable: bool) {
        let actions = AccountActions::init(self.context, is_valid_impl);
        actions.entrypoint(app_payload, fee_payment_method, cancellable);
    }

    #[external("private")]
    #[noinitcheck]
    #[view]
    fn verify_private_authwit(inner_hash: Field) -> Field {
        let actions = AccountActions::init(self.context, is_valid_impl);
        actions.verify_private_authwit(inner_hash)
    }

    /// Sets the spending threshold `k`. Only callable through an approved vault transaction.
    #[external("private")]
    #[only_self]
    fn set_threshold(threshold: u32) {
        self
            .storage
            .config
            .replace(|config| governance::with_threshold(config, threshold), self.address)
            .deliver(config_delivery(self.address));
    }

    /// Bumps the config version without changing the policy, invalidating every approval collected so far. Only
    /// callable through an approved vault transaction.
    #[external("private")]
    #[only_self]
    fn invalidate_pending() {
        self.storage.config.replace(governance::bump_version, self.address).deliver(config_delivery(
            self.address,
        ));
    }

    /// Current policy: thresholds, signer count, config version and commitments.
    #[external("utility")]
    unconstrained fn get_config() -> ConfigNote {
        self.storage.config.view_note()
    }

    /// The ordered signer set, rebuilt from signer notes. Members need it to assemble the auth witness.
    #[external("utility")]
    unconstrained fn get_signer_set() -> SignerSet {
        let config = self.storage.config.view_note();
        let notes = self.storage.signers.at(self.address).view_notes(NoteViewerOptions::new());

        let mut keys = [Point::empty(); MAX_SIGNERS];
        for i in 0..notes.len() {
            let note = notes.get(i);
            keys[note.slot] = Point { x: note.x, y: note.y };
        }
        SignerSet { keys, count: config.signer_count }
    }

    /// Whether the auth witness held by the PXE approves `inner_hash` for `consumer` and has not been consumed yet.
    ///
    /// Reverts when the witness is malformed (wrong signer set, invalid signature, expired) rather than returning
    /// `false`.
    #[external("utility")]
    unconstrained fn lookup_validity(consumer: AztecAddress, inner_hash: Field) -> bool {
        let config = self.storage.config.view_note();
        let block_header = self.context.block_header();
        let message_hash = compute_authwit_message_hash(
            consumer,
            self.context.chain_id(),
            self.context.version(),
            inner_hash,
        );

        let fields: [Field; WITNESS_LEN] = get_auth_witness(message_hash);
        let witness = ApprovalWitness::from_fields(fields, config.signer_count);
        let approved = authorization::check_witness(
            config,
            witness,
            self.address,
            message_hash,
            block_header.timestamp(),
        );

        let nullifier = compute_authwit_nullifier(self.address, inner_hash);
        let siloed_nullifier = compute_siloed_nullifier(consumer, nullifier);
        let (low_leaf_preimage, _witness) =
            get_low_nullifier_membership_witness(block_header, siloed_nullifier);
        let is_spent = low_leaf_preimage.nullifier == siloed_nullifier;

        approved & !is_spent
    }

    /// Constrained delivery for config notes: the executor acts for every member, and an undelivered config note would
    /// leave the others unable to prove.
    #[contract_library_method]
    fn config_delivery(vault: AztecAddress) -> OnchainConstrainedDelivery {
        MessageDelivery::onchain_constrained().with_sender(vault)
    }

    /// Inserts one signer note per member of `signers`.
    ///
    /// Unconstrained delivery: signer notes only help members rebuild the ordered set; authorization checks the
    /// commitment in the config note, and members also hold the set in their committee manifest.
    #[contract_library_method]
    fn store_signer_notes(notes: PrivateSet<SignerNote, &mut PrivateContext>, signers: SignerSet) {
        let delivery = MessageDelivery::onchain_unconstrained().with_sender(notes.owner);
        for slot in 0..MAX_SIGNERS {
            if slot < signers.count {
                let key = signers.keys[slot];
                notes.insert(SignerNote { x: key.x, y: key.y, slot }).deliver(delivery);
            }
        }
    }

    #[contract_library_method]
    fn is_valid_impl(context: &mut PrivateContext, outer_hash: Field) -> bool {
        let config_slot = Storage::init(context).config.get_storage_slot();
        authorization::is_authorized(context, config_slot, outer_hash)
    }
}
