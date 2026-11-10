//! Each member's keeper says what it speaks in the vault: its record reaches every member, the vault writes in the
//! version all of them speak (the first one, in this release), and a feature needing a newer version is refused with
//! the members holding it back.
#![allow(clippy::unwrap_used)]

mod support;

use std::time::Duration;

use ome_api::{RELEASE_VERSION, keeper::ProtocolRange};
use support::{fast_config, vault};

#[tokio::test(flavor = "multi_thread")]
async fn every_member_learns_what_the_others_run() {
    let v = vault(fast_config()).await;
    let alice = &v.keepers[0].keeper;
    // Each keeper announces itself on its first periodic round; the records replicate to every member.
    let mut protocol = alice.vault_protocol(v.address).await.unwrap();
    for _ in 0..100 {
        if protocol.members.iter().all(|m| m.release.is_some()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        protocol = alice.vault_protocol(v.address).await.unwrap();
    }
    assert_eq!(protocol.version, 0);
    assert_eq!(protocol.members.len(), 3);
    for member in &protocol.members {
        assert_eq!(member.release.as_deref(), Some(RELEASE_VERSION));
        assert_eq!(member.versions, Some(ProtocolRange { min: 0, max: 0 }));
    }

    // A feature that needs protocol v1 names every member, since none speaks it yet.
    let refused = alice
        .require_protocol(v.address, 1, "Batch payments")
        .await
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("Batch payments needs protocol v1"),
        "{refused}"
    );
    for index in 1..=3 {
        assert!(
            refused.contains(&format!("member {index} runs {RELEASE_VERSION}")),
            "{refused}"
        );
    }
    alice
        .require_protocol(v.address, 0, "Paying")
        .await
        .unwrap();
}
