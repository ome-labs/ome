//! The API tables are consistent and shapes round-trip as documented. CI regenerates `@ome-labs/api` and fails on a
//! diff, which keeps the TypeScript current.

use std::collections::HashSet;

use ome_api::{
    Amount, ErrorDetails, ErrorKind, Hex16, Hex32, HexBytes, NotReadyReason, RpcError, daemon,
    jsonrpc::Request, keeper, typescript,
};
use serde_json::json;

#[test]
fn method_names_are_unique_per_service() {
    for table in [keeper::METHODS, daemon::METHODS] {
        let mut seen = HashSet::new();
        for method in table {
            assert!(seen.insert(method.name), "{} appears twice", method.name);
        }
    }
}

#[test]
fn declared_type_names_are_unique() {
    let mut seen = HashSet::new();
    for name in typescript::declared_names() {
        assert!(seen.insert(name.clone()), "{name} is declared twice");
    }
}

#[test]
fn error_codes_are_distinct_and_round_trip() {
    let mut codes = HashSet::new();
    for kind in ErrorKind::ALL {
        assert!(codes.insert(kind.code()));
        assert_eq!(ErrorKind::from_code(kind.code()), kind);
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.label()));
    }
    let error: RpcError =
        serde_json::from_value(json!({ "code": -32011, "message": "locked" })).unwrap();
    assert_eq!(error.kind(), ErrorKind::Locked);
    // A kind from a later version reads as failed instead of breaking the reply.
    let error: RpcError = serde_json::from_value(
        json!({ "code": -32099, "message": "new", "data": { "kind": "quota" } }),
    )
    .unwrap();
    assert_eq!(error.kind(), ErrorKind::Failed);
    assert_eq!(
        serde_json::to_value(RpcError::new(ErrorKind::NotReady, "starting")).unwrap(),
        json!({ "code": -32010, "message": "starting", "data": { "kind": "not-ready" } })
    );
}

#[test]
fn details_carry_their_kind_and_unknown_ones_are_dropped() {
    let error = RpcError::detailed(
        ErrorDetails::Unreadable {
            answered: 1,
            needed: 2,
        },
        "only 1 of 2 keepers answered",
    );
    assert_eq!(error.kind(), ErrorKind::Unreadable);
    assert_eq!(
        serde_json::to_value(&error).unwrap(),
        json!({
            "code": -32020,
            "message": "only 1 of 2 keepers answered",
            "data": { "kind": "unreadable", "details": { "kind": "unreadable", "answered": 1, "needed": 2 } },
        })
    );
    assert_eq!(
        serde_json::from_value::<RpcError>(serde_json::to_value(&error).unwrap()).unwrap(),
        error
    );
    let ready = RpcError::detailed(
        ErrorDetails::NotReady {
            reason: NotReadyReason::Signer,
        },
        "unlock the signer first",
    );
    assert_eq!(
        serde_json::to_value(&ready).unwrap()["data"]["details"],
        json!({ "kind": "not-ready", "reason": "signer" })
    );
    // Details from a later version are dropped; the kind still reads.
    let error: RpcError = serde_json::from_value(json!({
        "code": -32020, "message": "later", "data": { "kind": "unreadable", "details": { "kind": "quota", "x": 1 } },
    }))
    .unwrap();
    assert_eq!(error.kind(), ErrorKind::Unreadable);
    assert_eq!(error.details(), None);
}

#[test]
fn details_belong_to_one_kind_each() {
    let kinds = [
        ErrorDetails::Expired { at: 1 }.kind(),
        ErrorDetails::Stale {
            made: 1,
            current: 2,
        }
        .kind(),
        ErrorDetails::Full { members: 3 }.kind(),
        ErrorDetails::UnknownContract {
            address: Hex32([1; 32]),
            class_id: None,
        }
        .kind(),
        ErrorDetails::Network {
            expected: "a".into(),
            actual: "b".into(),
        }
        .kind(),
    ];
    assert_eq!(
        kinds,
        [
            ErrorKind::Expired,
            ErrorKind::Stale,
            ErrorKind::Full,
            ErrorKind::UnknownContract,
            ErrorKind::Network
        ]
    );
}

#[test]
fn bytes_are_prefixed_hex() {
    let value = Hex32([0xab; 32]);
    let text = serde_json::to_value(value).unwrap();
    assert_eq!(text, json!(format!("0x{}", "ab".repeat(32))));
    assert_eq!(serde_json::from_value::<Hex32>(text).unwrap(), value);
    assert_eq!(
        serde_json::from_value::<Hex32>(json!("AB".repeat(32))).unwrap(),
        value
    );
    assert!(serde_json::from_value::<Hex32>(json!("0xab")).is_err());
    assert!(serde_json::from_value::<Hex16>(json!(format!("0x{}", "ab".repeat(32)))).is_err());
    assert_eq!(
        serde_json::to_value(HexBytes(vec![1, 2])).unwrap(),
        json!("0x0102")
    );
}

#[test]
fn amounts_are_decimal_strings() {
    let amount = Amount(u128::MAX);
    let text = serde_json::to_value(amount).unwrap();
    assert_eq!(text, json!(u128::MAX.to_string()));
    assert_eq!(serde_json::from_value::<Amount>(text).unwrap(), amount);
    for bad in [json!("-1"), json!("1.5"), json!(""), json!(25)] {
        assert!(serde_json::from_value::<Amount>(bad).is_err());
    }
}

#[test]
fn empty_params_accept_missing_null_and_empty() {
    for params in [None, Some(json!(null)), Some(json!({}))] {
        let mut request = json!({ "jsonrpc": "2.0", "id": 1, "method": "vault.list" });
        if let Some(params) = params {
            request["params"] = params;
        }
        let request: Request = serde_json::from_value(request).unwrap();
        request.params::<ome_api::Empty>().unwrap();
    }
    let request: Request = serde_json::from_value(
        json!({ "jsonrpc": "2.0", "id": 1, "method": "x", "params": { "a": 1 } }),
    )
    .unwrap();
    assert!(request.params::<ome_api::Empty>().is_err());
}

#[test]
fn events_are_tagged_by_type() {
    let event = keeper::KeeperEvent::UnexpectedSpend {
        vault: Hex32([1; 32]),
        tx_hash: Hex32([2; 32]),
    };
    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(value["type"], "unexpectedSpend");
    assert!(value.get("txHash").is_some());
    assert_eq!(
        serde_json::from_value::<keeper::KeeperEvent>(value).unwrap(),
        event
    );
    let event = daemon::DaemonEvent::KeeperEvent { event };
    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(value["type"], "keeperEvent");
    assert_eq!(value["event"]["type"], "unexpectedSpend");
}

#[test]
fn daemon_events_arrive_numbered_and_name_their_vault() {
    let vault = Hex32([7; 32]);
    let envelope = daemon::EventEnvelope {
        seq: daemon::EventSeq { epoch: 9, n: 3 },
        at: 100,
        event: daemon::DaemonEvent::BalanceChanged {
            vault,
            token: Hex32([8; 32]),
        },
    };
    let value = serde_json::to_value(&envelope).unwrap();
    assert_eq!(value["seq"], json!({ "epoch": 9, "n": 3 }));
    assert_eq!(value["event"]["type"], "balanceChanged");
    assert_eq!(
        serde_json::from_value::<daemon::EventEnvelope>(value).unwrap(),
        envelope
    );
    assert_eq!(envelope.event.vault(), Some(&vault));
    let subscribe: daemon::SubscribeParams = serde_json::from_value(json!({})).unwrap();
    assert_eq!(subscribe.since, None);
}

#[test]
fn typed_values_and_types_have_one_json_form_each() {
    use daemon::{AbiType, AbiValue, KnownType};
    let value: AbiValue = serde_json::from_value(json!({
        "amount": "1000", "path": ["1", "0x02"], "exact": true,
    }))
    .unwrap();
    let AbiValue::Object(fields) = &value else {
        panic!("a struct value is an object");
    };
    assert_eq!(fields["exact"], AbiValue::Bool(true));
    assert_eq!(
        fields["path"],
        AbiValue::List(vec![
            AbiValue::Text("1".into()),
            AbiValue::Text("0x02".into())
        ])
    );
    assert_eq!(
        serde_json::from_value::<AbiValue>(serde_json::to_value(&value).unwrap()).unwrap(),
        value
    );

    let address = AbiType::Struct {
        path: "aztec::protocol_types::address::aztec_address::AztecAddress".into(),
        fields: vec![],
        known: Some(KnownType::AztecAddress),
    };
    let array = AbiType::Array {
        length: 2,
        item: Box::new(AbiType::Integer {
            signed: false,
            width: 128,
        }),
    };
    assert_eq!(
        serde_json::to_value(&array).unwrap(),
        json!({ "kind": "array", "length": 2, "item": { "kind": "integer", "signed": false, "width": 128 } })
    );
    assert_eq!(
        serde_json::to_value(&address).unwrap()["known"],
        "aztecAddress"
    );
    let unknown = AbiType::Struct {
        path: "amm::Order".into(),
        fields: vec![],
        known: None,
    };
    assert!(
        serde_json::to_value(unknown)
            .unwrap()
            .get("known")
            .is_none()
    );
}

#[test]
fn ceremony_results_decode_by_shape() {
    let point = json!({ "x": format!("0x{}", "00".repeat(32)), "y": format!("0x{}", "00".repeat(31) + "02") });
    let dkg = json!({
        "kind": "dkg", "status": "completed", "failure": null,
        "result": { "index": 1, "threshold": 2, "groupKey": point, "verificationShares": [point],
                    "transcript": format!("0x{}", "11".repeat(64)) },
    });
    let ceremony: keeper::Ceremony = serde_json::from_value(dkg).unwrap();
    assert!(matches!(
        ceremony.result,
        Some(keeper::CeremonyResult::Dkg(_))
    ));
    let reshare = json!({
        "kind": "reshare", "status": "completed", "failure": null,
        "result": { "committeeCommitment": format!("0x{}", "22".repeat(32)), "newIndex": null,
                    "manifest": null, "transcript": null },
    });
    let ceremony: keeper::Ceremony = serde_json::from_value(reshare).unwrap();
    assert!(matches!(
        ceremony.result,
        Some(keeper::CeremonyResult::Reshare(_))
    ));
}

#[test]
fn optional_params_may_be_omitted() {
    let audit: keeper::AuditParams =
        serde_json::from_value(json!({ "vault": format!("0x{}", "00".repeat(32)) })).unwrap();
    assert_eq!(audit.limit, None);
    assert!(serde_json::to_value(audit).unwrap().get("limit").is_none());
    let secret: daemon::Secret =
        serde_json::from_value(json!({ "passphrase": "hunter22" })).unwrap();
    assert!(matches!(secret, daemon::Secret::Passphrase { .. }));
}
