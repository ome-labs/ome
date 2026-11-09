//! The preview stands in for the vault only if every function a proposal can call on the vault has the same selector
//! in both.

use crate::OmeVaultPreview;
use aztec::protocol::{address::AztecAddress, traits::FromField};
use ome_vault::{constants::MAX_SIGNERS, OmeVault, point::Point};

#[test]
fn the_preview_answers_the_vault_selectors() {
    let at = AztecAddress::from_field(0x1234);
    let keys = [Point::empty(); MAX_SIGNERS];

    assert_eq(
        OmeVaultPreview::at(at).rotate_membership(keys, keys, 0, 0, 0, keys, keys).selector,
        OmeVault::at(at).rotate_membership(keys, keys, 0, 0, 0, keys, keys).selector,
    );
    assert_eq(
        OmeVaultPreview::at(at).set_threshold(1).selector,
        OmeVault::at(at).set_threshold(1).selector,
    );
    assert_eq(
        OmeVaultPreview::at(at).invalidate_pending().selector,
        OmeVault::at(at).invalidate_pending().selector,
    );
    assert_eq(
        OmeVaultPreview::at(at).verify_private_authwit(1).selector,
        OmeVault::at(at).verify_private_authwit(1).selector,
    );
}
