//! Configurations the contract allows but members should choose knowingly. Clients show these as warnings; none of
//! them blocks a vault.

/// A risk in a vault's thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Warning {
    /// `k = 1`: any single member can spend alone.
    SingleSigner,
    /// `k = n`: losing any one member's key freezes the funds.
    NoSpareSigner,
    /// `t_v = 2` with three or more members: any two members, including two who later leave, can read everything the
    /// vault received while they were members.
    TwoCanRead,
    /// `t_v = n`: losing any one member's keeper makes the vault unreadable, and no rotation can recover it.
    NoSpareViewer,
}

/// The warnings for spending threshold `k`, viewing threshold `t_v` and `n` members.
#[must_use]
pub fn warnings(threshold: u32, viewing_threshold: u32, members: u32) -> Vec<Warning> {
    let mut found = Vec::new();
    if threshold == 1 && members > 1 {
        found.push(Warning::SingleSigner);
    }
    if threshold == members && members > 1 {
        found.push(Warning::NoSpareSigner);
    }
    if viewing_threshold == 2 && members >= 3 {
        found.push(Warning::TwoCanRead);
    }
    if viewing_threshold == members {
        found.push(Warning::NoSpareViewer);
    }
    found
}
