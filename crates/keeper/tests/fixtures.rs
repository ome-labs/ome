//! Keeper stores from earlier releases (`compat/fixtures/<release>/keeper.sqlite`) still open: migrated by this
//! release, every vault, proposal, vote, name and capabilities record in them reads back. Until launch
//! (`compat/MODE` is `report`) a problem is printed, not failed. `OME_UPDATE_FIXTURES=<release>` writes this release's
//! fixture from a real three-keeper vault.
#![allow(clippy::unwrap_used)]

mod support;

use std::{path::Path, time::Duration};

use ome_api::keeper::NameSubject;
use ome_keeper::store::Store;
use ome_wire::{Message, decode_body};
use rusqlite::Connection;
use support::{body, fast_config, vault};

const COMPAT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../compat");

/// Every record of every vault in the store at `path`, read through this release.
fn read_everything(path: &Path) -> Result<usize, String> {
    let copy = tempfile::tempdir().unwrap();
    let opened = copy.path().join("keeper.sqlite");
    std::fs::copy(path, &opened).unwrap();
    let store = Store::open(&opened).map_err(|e| format!("does not open: {e}"))?;
    let mut records = 0;
    for (manifest, _) in store.vaults().map_err(|e| format!("vaults: {e}"))? {
        let vault = manifest.vault;
        let mut bodies = store
            .proposals(&vault)
            .map_err(|e| format!("proposals: {e}"))?;
        for proposal in bodies.clone() {
            let Ok(Message::Proposal(p)) = decode_body(&proposal) else {
                return Err("a proposal no longer decodes".into());
            };
            let id = p.id().map_err(|e| e.to_string())?;
            bodies.extend(
                store
                    .approvals(&id)
                    .map_err(|e| format!("approvals: {e}"))?,
            );
            bodies.extend(
                store
                    .rejections(&id)
                    .map_err(|e| format!("rejections: {e}"))?,
            );
        }
        bodies.extend(store.names(&vault).map_err(|e| format!("names: {e}"))?);
        bodies.extend(
            store
                .capabilities(&vault)
                .map_err(|e| format!("capabilities: {e}"))?,
        );
        for record in &bodies {
            decode_body(record).map_err(|e| format!("a record no longer decodes: {e}"))?;
        }
        records += bodies.len() + 1;
    }
    Ok(records)
}

#[test]
fn stores_from_earlier_releases_still_read() {
    let enforce = std::fs::read_to_string(format!("{COMPAT}/MODE"))
        .unwrap()
        .trim()
        == "enforce";
    let Ok(releases) = std::fs::read_dir(format!("{COMPAT}/fixtures")) else {
        return;
    };
    let mut problems = Vec::new();
    for release in releases {
        let path = release.unwrap().path().join("keeper.sqlite");
        if !path.exists() {
            continue;
        }
        match read_everything(&path) {
            Ok(0) => problems.push(format!("{}: holds no vault", path.display())),
            Ok(_) => {}
            Err(problem) => problems.push(format!("{}: {problem}", path.display())),
        }
    }
    let report = problems.join("\n");
    assert!(
        !enforce || problems.is_empty(),
        "an earlier release's store no longer reads:\n{report}"
    );
    if !problems.is_empty() {
        eprintln!("compat (report mode, v0 still open):\n{report}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn write_this_releases_fixture() {
    let Some(release) = std::env::var_os("OME_UPDATE_FIXTURES") else {
        return;
    };
    let v = vault(fast_config()).await;
    let alice = &v.keepers[0].keeper;
    let (proposal, payload) = v.proposal(25);
    alice
        .put_payload(v.address, &body(&Message::Payload(payload)))
        .await
        .unwrap();
    alice
        .put_proposal(v.address, &body(&Message::Proposal(proposal.clone())))
        .await
        .unwrap();
    alice
        .put_approval(
            v.address,
            &body(&Message::Approval(v.approval(&proposal, 0, 0))),
        )
        .await
        .unwrap();
    alice
        .put_name(v.address, NameSubject::Vault, "Treasury".into())
        .await
        .unwrap();
    for _ in 0..100 {
        if alice
            .vault_protocol(v.address)
            .await
            .unwrap()
            .members
            .iter()
            .all(|m| m.release.is_some())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let dir = Path::new(COMPAT).join("fixtures").join(release);
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("keeper.sqlite");
    let _ = std::fs::remove_file(&target);
    Connection::open(v.keepers[0].dir.path().join("keeper.db"))
        .unwrap()
        .execute("VACUUM INTO ?1", [target.to_string_lossy()])
        .unwrap();
    assert!(read_everything(&target).unwrap() > 1);
}
