//! A deployed OmeVault in the TXE, driven the way members' PXEs drive it: views with the vault in scope, governance as
//! calls from the vault itself, and authorization checks served a chosen auth witness.

use crate::fixtures::{
    approvals::{encode_witness, Signatures},
    members::Members,
    points::{distinct_points, FOUNDING_SHARES, ROTATED_SHARES},
};
use aztec::{
    context::{PrivateCall, PrivateContext, UtilityCall},
    protocol::{address::AztecAddress, traits::Deserialize},
    test::helpers::{
        test_environment::{CallPrivateOptions, ExecuteUtilityOptions, TestEnvironment},
        txe_oracles,
    },
};
use ome_vault::{
    authorization::{approval_message, is_authorized},
    constants::{MAX_SIGNERS, SIGNATURE_LEN, WITNESS_LEN},
    notes::config_note::ConfigNote,
    OmeVault,
    point::Point,
    signer_set::SignerSet,
};
use std::test::OracleMock;
use test_signer::TestSigner;

/// The hash an authorization test approves. The vault signs over any field, so the value only has to be fixed.
pub global OUTER_HASH: Field = 0x7ec9a2;

pub global ONE_DAY: u64 = 86400;

/// A deployed vault, its current members and policy, and the helper contract that exposes TXE signatures.
pub struct TestVault {
    pub env: TestEnvironment,
    pub address: AztecAddress,
    pub signer: AztecAddress,
    pub members: Members,
    pub committee_shares: [Point; MAX_SIGNERS],
    pub threshold: u32,
    pub viewing_threshold: u32,
}

/// Deploys a vault with the given constructor arguments and returns its address.
///
/// Like an Aztec.js account deployment, the deployer's call has the vault in scope so the constructor can use the
/// vault's own keys (`with_private_initializer` would only scope the call to the deployer).
pub unconstrained fn deploy_vault(
    env: &mut TestEnvironment,
    keys: [Point; MAX_SIGNERS],
    signer_count: u32,
    threshold: u32,
    viewing_threshold: u32,
    committee_shares: [Point; MAX_SIGNERS],
) -> AztecAddress {
    let deployer = env.create_light_account();
    let deployment = env.deploy("@ome_vault/OmeVault");
    let constructor = OmeVault::interface().constructor(
        keys,
        signer_count,
        threshold,
        viewing_threshold,
        committee_shares,
    );

    let vault = txe_oracles::deploy(
        deployment.path,
        constructor.name,
        constructor.args,
        deployment.secret,
        deployment.salt,
        AztecAddress::zero(),
        deployment.immutables_hash,
    )
        .to_address();

    let _ = env.call_private_opts(
        deployer,
        CallPrivateOptions::new().with_additional_scopes([vault]),
        PrivateCall::<_, _, ()>::new(
            vault,
            constructor.selector,
            constructor.name,
            constructor.args,
        ),
    );
    vault
}

impl TestVault {
    /// A `threshold`-of-`member_count` vault with viewing threshold `viewing_threshold` and fresh members.
    pub unconstrained fn found(member_count: u32, threshold: u32, viewing_threshold: u32) -> Self {
        let mut env = TestEnvironment::new();
        let members = Members::create(&mut env, member_count);
        Self::found_with(env, members, threshold, viewing_threshold)
    }

    /// A vault for `members`, created in `env`.
    pub unconstrained fn found_with(
        mut env: TestEnvironment,
        members: Members,
        threshold: u32,
        viewing_threshold: u32,
    ) -> Self {
        let committee_shares = distinct_points(members.count, FOUNDING_SHARES);
        let address = deploy_vault(
            &mut env,
            members.keys,
            members.count,
            threshold,
            viewing_threshold,
            committee_shares,
        );
        let signer = env.deploy("@test_signer/TestSigner").without_initializer();

        let vault =
            Self { env, address, signer, members, committee_shares, threshold, viewing_threshold };

        // A PXE discovers the vault's notes before simulating a transaction. Inline private contexts skip that step,
        // so sync them once with a utility call.
        let _ = vault.config();
        vault
    }

    /// The stored policy, read as a member's PXE reads it.
    pub unconstrained fn config(self) -> ConfigNote {
        self.view(OmeVault::at(self.address).get_config())
    }

    /// The signer set rebuilt from the vault's signer notes.
    pub unconstrained fn signer_set(self) -> SignerSet {
        self.view(OmeVault::at(self.address).get_signer_set())
    }

    /// Runs a utility function with the vault in scope.
    pub unconstrained fn view<let M: u32, let N: u32, T>(self, call: UtilityCall<M, N, T>) -> T
    where
        T: Deserialize,
    {
        self.env.execute_utility_opts(ExecuteUtilityOptions::new().with_from(self.address), call)
    }

    /// Calls a vault function as the vault itself, as its `entrypoint` does after approval, then syncs its notes.
    pub unconstrained fn self_call<let M: u32, let N: u32>(self, call: PrivateCall<M, N, ()>) {
        self.env.call_private(self.address, call);
        let _ = self.config();
    }

    /// Rotates to `members` with fresh committee shares and returns the vault under its new policy.
    pub unconstrained fn rotate_to(
        self,
        members: Members,
        threshold: u32,
        viewing_threshold: u32,
    ) -> Self {
        let committee_shares = distinct_points(
            members.count,
            ROTATED_SHARES + self.config().version as Field,
        );

        self.self_call(OmeVault::at(self.address).rotate_membership(
            self.members.keys,
            members.keys,
            members.count,
            threshold,
            viewing_threshold,
            committee_shares,
        ));

        let mut rotated = self;
        rotated.members = members;
        rotated.committee_shares = committee_shares;
        rotated.threshold = threshold;
        rotated.viewing_threshold = viewing_threshold;
        rotated
    }

    /// Runs the vault's constrained authorization check for `outer_hash`, serving `witness` as the auth witness.
    pub unconstrained fn authorize(self, outer_hash: Field, witness: [Field; WITNESS_LEN]) -> bool {
        self.in_private_context(witness, |context| {
            let config_slot = OmeVault::storage_layout().config.slot;
            is_authorized(context, config_slot, outer_hash)
        })
    }

    /// The transaction expiration an authorization of `outer_hash` with `witness` sets.
    pub unconstrained fn authorized_expiration(
        self,
        outer_hash: Field,
        witness: [Field; WITNESS_LEN],
    ) -> u64 {
        self.in_private_context(witness, |context| {
            let config_slot = OmeVault::storage_layout().config.slot;
            assert(is_authorized(context, config_slot, outer_hash));
            context.finish().expiration_timestamp
        })
    }

    /// Runs `check` in a private context at the vault while the PXE serves `witness` once.
    unconstrained fn in_private_context<Env, T>(
        self,
        witness: [Field; WITNESS_LEN],
        check: unconstrained fn[Env](&mut PrivateContext) -> T,
    ) -> T {
        // One answer per check: an open-ended mock would keep answering later checks in the same test.
        let _ = OracleMock::mock("aztec_utl_getAuthWitness").returns(witness).times(1);
        self.env.private_context_at(self.address, check)
    }

    /// Witness approving `OUTER_HASH` under `config_version` from the `approving` slots, valid for a day.
    pub unconstrained fn approve(
        self,
        config_version: u32,
        approving: [bool; MAX_SIGNERS],
    ) -> [Field; WITNESS_LEN] {
        let expires_at = self.now() + ONE_DAY;
        let message = self.approval_message(OUTER_HASH, config_version, expires_at);
        self.witness(message, approving, expires_at)
    }

    /// Witness carrying the vault's signer keys, an approval of `message` from each `approving` slot, and `expires_at`.
    pub unconstrained fn witness(
        self,
        message: Field,
        approving: [bool; MAX_SIGNERS],
        expires_at: u64,
    ) -> [Field; WITNESS_LEN] {
        encode_witness(
            self.members.keys,
            self.signatures(message, approving),
            expires_at,
        )
    }

    /// Signatures of `message` from each `approving` slot; other slots stay empty.
    pub unconstrained fn signatures(
        self,
        message: Field,
        approving: [bool; MAX_SIGNERS],
    ) -> Signatures {
        let mut signatures = [[0; SIGNATURE_LEN]; MAX_SIGNERS];
        for i in 0..MAX_SIGNERS {
            if approving[i] {
                signatures[i] = self.sign(self.members.accounts[i], message);
            }
        }
        signatures
    }

    /// Schnorr signature of `message` by `account`.
    pub unconstrained fn sign(
        self,
        account: AztecAddress,
        message: Field,
    ) -> [Field; SIGNATURE_LEN] {
        txe_oracles::add_authwit(account, message);
        self.env.call_private(account, TestSigner::at(self.signer).signature(message))
    }

    /// The message members sign to approve `outer_hash` under `config_version` until `expires_at`.
    pub unconstrained fn approval_message(
        self,
        outer_hash: Field,
        config_version: u32,
        expires_at: u64,
    ) -> Field {
        approval_message(self.address, outer_hash, config_version, expires_at)
    }

    /// Timestamp of the latest block, which inline private contexts use as their anchor.
    pub unconstrained fn now(self) -> u64 {
        self.env.last_block_timestamp()
    }
}
