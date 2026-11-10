//! What the keeper does, each an `impl Keeper` block: ceremonies, vault custody, threshold ECDH, proposals and their
//! replication, sync, mailboxes, lobbies, paired devices and peer reachability.

pub(crate) mod artifacts;
pub(crate) mod ceremonies;
pub(crate) mod claims;
pub(crate) mod devices;
pub(crate) mod ecdh;
pub(crate) mod lobby;
pub(crate) mod mailbox;
pub(crate) mod peers;
pub(crate) mod proposals;
pub(crate) mod sync;
pub(crate) mod vaults;
