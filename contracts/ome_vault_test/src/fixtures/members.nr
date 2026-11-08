//! Vault members as the TXE sees them: an account that signs and the Grumpkin key the vault stores for it.

use aztec::{
    keys::getters::get_public_keys, protocol::address::AztecAddress,
    test::helpers::test_environment::TestEnvironment,
};
use ome_vault::{constants::MAX_SIGNERS, point::Point};

/// One signing member.
pub struct Member {
    pub account: AztecAddress,
    pub key: Point,
}

/// Ordered members in slots `0..count`; later slots are empty.
pub struct Members {
    pub accounts: [AztecAddress; MAX_SIGNERS],
    pub keys: [Point; MAX_SIGNERS],
    pub count: u32,
}

/// Creates a TXE account and reads its signing key.
///
/// The TXE signs auth witnesses with an account's incoming viewing secret, so `ivpk_m` is the key a vault must store.
pub unconstrained fn new_member(env: &mut TestEnvironment) -> Member {
    let account = env.create_light_account();
    let key = env.utility_context(|_context| get_public_keys(account).ivpk_m.inner);
    Member { account, key: Point { x: key.x, y: key.y } }
}

impl Members {
    /// `count` fresh members.
    pub unconstrained fn create(env: &mut TestEnvironment, count: u32) -> Self {
        let mut members = Members {
            accounts: [AztecAddress::zero(); MAX_SIGNERS],
            keys: [Point::empty(); MAX_SIGNERS],
            count,
        };

        for i in 0..count {
            let member = new_member(env);
            members.accounts[i] = member.account;
            members.keys[i] = member.key;
        }

        members
    }

    /// The member in `slot`.
    pub fn at(self, slot: u32) -> Member {
        Member { account: self.accounts[slot], key: self.keys[slot] }
    }

    /// `member` takes over `slot` (a new device, or a different person in the same seat).
    pub fn replacing(self, slot: u32, member: Member) -> Self {
        let mut next = self;
        next.accounts[slot] = member.account;
        next.keys[slot] = member.key;
        next
    }

    /// `member` joins in the first empty slot.
    pub fn joined_by(self, member: Member) -> Self {
        let mut next = self.replacing(self.count, member);
        next.count += 1;
        next
    }

    /// The member in `slot` leaves; later members move down one slot so the set stays packed.
    pub fn without(self, slot: u32) -> Self {
        let mut next = self;
        for i in 0..MAX_SIGNERS - 1 {
            if i >= slot {
                next.accounts[i] = self.accounts[i + 1];
                next.keys[i] = self.keys[i + 1];
            }
        }

        next.accounts[MAX_SIGNERS - 1] = AztecAddress::zero();
        next.keys[MAX_SIGNERS - 1] = Point::empty();
        next.count -= 1;

        next
    }

    /// The members in slots `a` and `b` trade places.
    pub fn swapped(self, a: u32, b: u32) -> Self {
        self.replacing(a, self.at(b)).replacing(b, self.at(a))
    }
}
