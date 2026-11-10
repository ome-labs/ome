//! Founding: a join proof verifies only with the invite secret and the joiner's own keys, founding secrets open only
//! for their recipient, and the vault's version is one every founder speaks.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::sealing::envelope::TransportSecret;
use ome_protocol::{
    Error,
    lobby::{join_proof, open_secrets, plan_version, seal_secrets, verify_join},
};
use ome_wire::{
    IdentityPublic,
    messages::lobby::{JoinSecret, KeeperContact, LobbyJoin, LobbyMember},
    types::WirePoint,
    version::{SUPPORTED, VersionRange},
};
use support::seeded;

fn member(n: u8) -> LobbyMember {
    LobbyMember {
        name: format!("member {n}"),
        account: [n; 32],
        signer_key: WirePoint::from(&ome_crypto::Point::generator()),
        keeper: KeeperContact {
            identity: IdentityPublic([n; 32]),
            transport: [n + 100; 32],
            versions: SUPPORTED,
            addresses: vec![],
        },
    }
}

fn join(secret: &JoinSecret, member: LobbyMember) -> LobbyJoin {
    LobbyJoin {
        lobby: [1; 16],
        proof: join_proof(secret, &[1; 16], &member.keeper),
        member,
    }
}

#[test]
fn a_join_verifies_only_with_the_invite_secret_and_the_joiners_keys() {
    let secret = JoinSecret([5; 32]);
    let request = join(&secret, member(2));
    let signer = IdentityPublic([2; 32]);

    assert_eq!(verify_join(&request, &secret, &signer), Ok(()));
    assert_eq!(
        verify_join(&request, &JoinSecret([6; 32]), &signer),
        Err(Error::InvalidJoinProof)
    );
    assert_eq!(
        verify_join(&request, &secret, &IdentityPublic([3; 32])),
        Err(Error::UnexpectedSigner)
    );

    let mut swapped_transport = request.clone();
    swapped_transport.member.keeper.transport = [0; 32];
    assert_eq!(
        verify_join(&swapped_transport, &secret, &signer),
        Err(Error::InvalidJoinProof)
    );
}

#[test]
fn the_vault_version_is_one_every_founder_speaks() {
    let mut behind = member(3);
    behind.keeper.versions = VersionRange { min: 1, max: 2 };

    assert_eq!(plan_version(&[member(1), member(2)]), Ok(SUPPORTED.max));
    assert_eq!(
        plan_version(&[member(1), behind]),
        Err(Error::Wire(ome_wire::Error::NoCommonVersion))
    );
}

#[test]
fn founding_secrets_open_only_for_their_recipient() {
    let mut rng = seeded(3);
    let (host, host_public) = TransportSecret::generate(&mut rng);
    let (guest, guest_public) = TransportSecret::generate(&mut rng);
    let session = [8; 32];

    let sealed = seal_secrets(
        &session,
        2,
        &host,
        &guest_public,
        b"founding secrets",
        &mut rng,
    )
    .unwrap();

    assert_eq!(
        open_secrets(&session, 2, &guest, &host_public, &sealed)
            .unwrap()
            .as_slice(),
        b"founding secrets"
    );
    assert!(open_secrets(&session, 3, &guest, &host_public, &sealed).is_err());
    assert!(open_secrets(&[0; 32], 2, &guest, &host_public, &sealed).is_err());
}
