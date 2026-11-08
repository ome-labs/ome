//! Config transitions. Each returns the next `ConfigNote` and asserts the invariants: `1 <= k <= n`,
//! `2 <= t_v <= n`, the version increases by one, and a rotation removes fewer than the current `t_v` members.

use crate::{
    committee::Committee, constants::MAX_SIGNERS, notes::config_note::ConfigNote, point::Point,
    signer_set::SignerSet,
};

/// Same policy, next version: invalidates every approval collected so far.
pub fn bump_version(config: ConfigNote) -> ConfigNote {
    ConfigNote {
        threshold: config.threshold,
        signer_count: config.signer_count,
        version: config.version + 1,
        signer_set_commitment: config.signer_set_commitment,
        viewing_threshold: config.viewing_threshold,
        committee_commitment: config.committee_commitment,
    }
}

/// New spending threshold `k`; the signer set and committee are unchanged.
pub fn with_threshold(config: ConfigNote, threshold: u32) -> ConfigNote {
    assert_threshold(threshold, config.signer_count);
    ConfigNote {
        threshold,
        signer_count: config.signer_count,
        version: config.version + 1,
        signer_set_commitment: config.signer_set_commitment,
        viewing_threshold: config.viewing_threshold,
        committee_commitment: config.committee_commitment,
    }
}

/// New signer set, spending threshold and viewing committee, as one atomic change.
///
/// `current_keys` must be the stored signer set (checked against its commitment). At most `t_v - 1` current members
/// may be absent from the new set, so the viewing reshare that accompanies the rotation keeps the viewing secret safe.
pub fn rotate(
    config: ConfigNote,
    current_keys: [Point; MAX_SIGNERS],
    new_keys: [Point; MAX_SIGNERS],
    new_count: u32,
    threshold: u32,
    viewing_threshold: u32,
    committee_shares: [Point; MAX_SIGNERS],
) -> ConfigNote {
    let current = SignerSet { keys: current_keys, count: config.signer_count };
    assert(current.commitment() == config.signer_set_commitment, "current signer set mismatch");

    let next = SignerSet::new(new_keys, new_count);
    assert_threshold(threshold, new_count);
    let committee = Committee::new(viewing_threshold, committee_shares, new_count);
    assert(removed_members(current, next) < config.viewing_threshold, "too many members removed");

    ConfigNote {
        threshold,
        signer_count: new_count,
        version: config.version + 1,
        signer_set_commitment: next.commitment(),
        viewing_threshold,
        committee_commitment: committee.commitment(),
    }
}

/// Number of occupied slots in an ordered, zero-padded key list.
pub fn member_count(keys: [Point; MAX_SIGNERS]) -> u32 {
    let mut count: u32 = 0;
    for i in 0..MAX_SIGNERS {
        count += (!keys[i].is_empty()) as u32;
    }
    count
}

/// Number of members of `current` whose key does not appear in `next`.
pub fn removed_members(current: SignerSet, next: SignerSet) -> u32 {
    let mut removed: u32 = 0;
    for i in 0..MAX_SIGNERS {
        if i < current.count {
            let mut kept = false;
            for j in 0..MAX_SIGNERS {
                kept |= (j < next.count) & (next.keys[j] == current.keys[i]);
            }
            removed += (!kept) as u32;
        }
    }
    removed
}

/// Asserts the spending threshold invariant `1 <= k <= n`.
pub fn assert_threshold(threshold: u32, signer_count: u32) {
    assert((threshold >= 1) & (threshold <= signer_count), "threshold out of range");
}
