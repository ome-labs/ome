//! Frozen protocol versions keep reading: every message in `compat/vectors/v<N>.json` still decodes with this code,
//! verifies, and re-encodes to the same bytes, and the current version's vectors equal its frozen ones. Until launch
//! (`compat/MODE` is `report`) a difference is printed, not failed: protocol `v0` is still open.
#![allow(clippy::unwrap_used)]

use ome_wire::{decode, encode, messages::lobby::Invite, version::PROTOCOL_VERSION};
use serde_json::Value;

const COMPAT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../compat");
const CURRENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vectors/wire.json");

fn enforced() -> bool {
    std::fs::read_to_string(format!("{COMPAT}/MODE"))
        .unwrap()
        .trim()
        == "enforce"
}

fn check(problems: &[String]) {
    if problems.is_empty() {
        return;
    }
    let report = problems.join("\n");
    assert!(
        !enforced(),
        "a frozen protocol version no longer reads as it did:\n{report}\nAdd a protocol version instead."
    );
    eprintln!("compat (report mode, v0 still open):\n{report}");
}

#[test]
fn every_frozen_message_still_decodes_verifies_and_reencodes() {
    let mut problems = Vec::new();
    for entry in std::fs::read_dir(format!("{COMPAT}/vectors")).unwrap() {
        let path = entry.unwrap().path();
        let frozen: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        for (kind, hex) in frozen["messages"].as_object().unwrap() {
            let bytes = hex::decode(hex.as_str().unwrap()).unwrap();
            match decode(&bytes) {
                Ok(message) => {
                    if message.verify().is_err() {
                        problems.push(format!("{name} {kind}: no longer verifies"));
                    }
                    if encode(&message).ok().as_deref() != Some(bytes.as_slice()) {
                        problems.push(format!("{name} {kind}: re-encodes to other bytes"));
                    }
                }
                Err(error) => problems.push(format!("{name} {kind}: no longer decodes ({error})")),
            }
        }
        if let Some(invite) = frozen["invite"].as_str()
            && let Err(error) = Invite::decode(invite)
        {
            problems.push(format!("{name} invite: no longer decodes ({error})"));
        }
    }
    check(&problems);
}

#[test]
fn the_current_version_still_encodes_as_frozen() {
    let path = format!("{COMPAT}/vectors/v{PROTOCOL_VERSION}.json");
    let Ok(frozen) = std::fs::read_to_string(&path) else {
        check(&[format!(
            "protocol v{PROTOCOL_VERSION} has no frozen vectors ({path})"
        )]);
        return;
    };
    let frozen: Value = serde_json::from_str(&frozen).unwrap();
    let current: Value = serde_json::from_str(&std::fs::read_to_string(CURRENT).unwrap()).unwrap();
    let mut problems = Vec::new();
    for key in ["messages", "invite", "pairOffer", "mailbox"] {
        if frozen[key] != current[key] {
            problems.push(format!(
                "v{PROTOCOL_VERSION} {key}: the encoding changed without a new protocol version"
            ));
        }
    }
    check(&problems);
}
