//! The contract mirror equals the `OmeVault` contract: the same vectors are pinned in the Noir tests, so a change on
//! either side fails both.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::Point;
use ome_protocol::vault::{
    Committee, SignerSet, approval_message,
    constants::{APPROVAL_TAG, COMMITTEE_TAG, SIGNER_SET_TAG},
    tag,
};
use serde_json::Value;
use support::vectors::{base, contract, point};

fn points(list: &Value) -> Vec<Point> {
    list.as_array().unwrap().iter().filter_map(point).collect()
}

#[test]
fn domain_tags_match_the_contract() {
    let v = contract();

    for (key, name) in [
        ("approval", APPROVAL_TAG),
        ("signers", SIGNER_SET_TAG),
        ("committee", COMMITTEE_TAG),
    ] {
        assert_eq!(v["tags"][key]["name"], name);
        assert_eq!(tag(name), base(&v["tags"][key]["field"]), "{name}");
    }
}

#[test]
fn signer_set_commitments_match_the_contract() {
    for case in contract()["signerSets"].as_array().unwrap() {
        let signers = SignerSet::new(points(&case["keys"])).unwrap();

        assert_eq!(u64::from(signers.count()), case["count"].as_u64().unwrap());
        assert_eq!(signers.commitment(), base(&case["commitment"]));
    }
}

#[test]
fn committee_commitments_match_the_contract() {
    for case in contract()["committees"].as_array().unwrap() {
        let threshold = u32::try_from(case["viewingThreshold"].as_u64().unwrap()).unwrap();
        let committee = Committee::new(threshold, points(&case["shares"])).unwrap();

        assert_eq!(committee.commitment(), base(&case["commitment"]));
    }
}

#[test]
fn approval_messages_match_the_contract() {
    for case in contract()["approvalMessages"].as_array().unwrap() {
        let message = approval_message(
            base(&case["vault"]),
            base(&case["outerHash"]),
            u32::try_from(case["configVersion"].as_u64().unwrap()).unwrap(),
            case["expiresAt"].as_str().unwrap().parse().unwrap(),
        );

        assert_eq!(message, base(&case["message"]));
    }
}
