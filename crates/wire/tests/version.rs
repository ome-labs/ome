//! Version negotiation: parties settle on the newest version they all speak, or fail clearly.

use ome_wire::{
    Error, KeeperIdentity,
    messages::capabilities::Capabilities,
    version::{SUPPORTED, VersionRange, holding_back, negotiate, require_supported, vault_version},
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

#[test]
fn a_vault_writes_the_newest_version_every_member_speaks() {
    let range = |min, max| Some(range(min, max));
    // Every member speaks the first version; a member that never said counts as speaking only it.
    assert_eq!(vault_version([range(0, 0), range(0, 0)]), Ok(0));
    assert_eq!(vault_version([range(0, 0), None]), Ok(0));
    // A member can't take the vault past what this build speaks.
    assert_eq!(vault_version([range(0, 7), range(0, 7)]), Ok(SUPPORTED.max));
    assert_eq!(vault_version([range(3, 5)]), Err(Error::NoCommonVersion));

    let members = [
        ("alice", range(0, 1)),
        ("bob", range(0, 0)),
        ("carol", None),
    ];
    assert_eq!(holding_back(1, &members), ["bob", "carol"]);
    assert_eq!(holding_back(0, &members), Vec::<&str>::new());
}

#[test]
fn a_capabilities_record_verifies_only_as_signed() {
    let identity = KeeperIdentity::from_seed(&[4; 32]);
    let record = Capabilities::sign(&identity, [7; 32], SUPPORTED, "0.3.0".into(), 10).unwrap();
    record.verify().unwrap();
    let edited = Capabilities {
        release: "9.9.9".into(),
        ..record.clone()
    };
    assert_eq!(edited.verify(), Err(Error::InvalidSignature));
    let newer = Capabilities::sign(&identity, [7; 32], SUPPORTED, "0.3.1".into(), 11).unwrap();
    assert!(newer.supersedes(&record) && !record.supersedes(&newer));
    assert!(Capabilities::sign(&identity, [7; 32], SUPPORTED, String::new(), 10).is_err());
    assert!(Capabilities::sign(&identity, [7; 32], SUPPORTED, "x".repeat(33), 10).is_err());
}

#[test]
fn a_pasted_string_in_a_newer_format_asks_for_an_update() {
    use ome_wire::version::strip_prefix;
    assert_eq!(
        strip_prefix("ome-invite-v0:abc", "ome-invite-v0:", "invite"),
        Ok("abc")
    );
    assert_eq!(
        strip_prefix("ome-invite-v3:abc", "ome-invite-v0:", "invite"),
        Err(Error::NewerFormat {
            what: "invite",
            version: 3
        })
    );
    assert_eq!(
        strip_prefix("ome-join-v0:abc", "ome-invite-v0:", "invite"),
        Err(Error::Malformed)
    );
    assert_eq!(
        strip_prefix("ome-invite-vx:abc", "ome-invite-v0:", "invite"),
        Err(Error::Malformed)
    );
    let error = ome_wire::messages::lobby::Invite::decode("ome-invite-v2:AAAA").unwrap_err();
    assert_eq!(
        error.to_string(),
        "this invite needs a newer release of Ome (format v2)"
    );
}
