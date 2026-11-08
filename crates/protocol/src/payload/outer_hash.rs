use ome_crypto::{Base, poseidon2};

use crate::vault::constants::DOM_SEP_AUTHWIT_OUTER;

/// The chain a transaction is bound to: the L1 chain id and the rollup version from the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainInfo {
    /// L1 chain id.
    pub chain_id: Base,
    /// Rollup protocol version.
    pub version: Base,
}

/// The authwit message hash the vault authorizes: `Poseidon2(AUTHWIT_OUTER, [consumer, chain_id, version, inner])`.
/// For the vault's own entrypoint, `consumer` is the vault and `inner` the app payload hash.
#[must_use]
pub fn outer_hash(consumer: Base, chain: ChainInfo, inner_hash: Base) -> Base {
    poseidon2::hash_with_separator(
        Base::from(DOM_SEP_AUTHWIT_OUTER),
        &[consumer, chain.chain_id, chain.version, inner_hash],
    )
}
