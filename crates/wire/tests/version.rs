//! Version negotiation: parties settle on the newest version they all speak, or fail clearly.

use ome_wire::{
    Error,
    version::{SUPPORTED, VersionRange, negotiate, require_supported},
};

const fn range(min: u16, max: u16) -> VersionRange {
    VersionRange { min, max }
}

#[test]
fn picks_the_newest_version_every_party_speaks() {
    assert_eq!(negotiate(&[range(0, 3), range(1, 2), range(0, 5)]), Ok(2));
    assert_eq!(negotiate(&[range(4, 4)]), Ok(4));
}

#[test]
fn fails_without_a_common_version() {
    assert_eq!(
        negotiate(&[range(0, 1), range(2, 3)]),
        Err(Error::NoCommonVersion)
    );
    assert_eq!(negotiate(&[]), Err(Error::NoCommonVersion));
}

#[test]
fn rejects_an_inverted_range() {
    assert_eq!(
        negotiate(&[SUPPORTED, range(3, 1)]),
        Err(Error::OutOfBounds { field: "versions" })
    );
}

#[test]
fn this_build_speaks_exactly_its_supported_range() {
    assert_eq!(require_supported(SUPPORTED.max), Ok(()));
    assert_eq!(
        require_supported(SUPPORTED.max + 1),
        Err(Error::UnsupportedVersion(SUPPORTED.max + 1))
    );
}
