//! Payload hashing equals Aztec's entrypoint encoder, so members sign exactly the calls they review, and a call's
//! arguments split by its signature for display.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::Base;
use ome_protocol::{
    Error,
    payload::{
        Call, CallKind, ChainInfo, ExecutionRequest, app_payload_fields, app_payload_hash,
        arguments, outer_hash, selector,
    },
};
use ome_wire::{messages::proposals::ProposalPayload, types::field};
use serde_json::Value;
use support::{
    fixture::request,
    vectors::{base, payload},
};

fn call(spec: &Value) -> Call {
    let signature = spec["signature"].as_str().unwrap();
    Call {
        signature: Some(signature.to_owned()),
        to: field(&base(&spec["to"])),
        selector: selector(signature).unwrap(),
        kind: if spec["type"] == "public" {
            CallKind::Public
        } else {
            CallKind::Private
        },
        is_static: spec["isStatic"].as_bool().unwrap(),
        hide_msg_sender: spec["hideMsgSender"].as_bool().unwrap(),
        args: spec["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| field(&base(a)))
            .collect(),
    }
}

fn bare(kind: CallKind, selector: u32, args: &[Value]) -> Call {
    Call {
        signature: None,
        to: field(&Base::from(1u64)),
        selector,
        kind,
        is_static: false,
        hide_msg_sender: false,
        args: args.iter().map(|a| field(&base(a))).collect(),
    }
}

#[test]
fn selectors_match_aztec() {
    for case in payload()["selectors"].as_array().unwrap() {
        let expected = u32::from_str_radix(
            case["selector"].as_str().unwrap().trim_start_matches("0x"),
            16,
        )
        .unwrap();
        assert_eq!(selector(case["signature"].as_str().unwrap()), Ok(expected));
    }
    assert_eq!(
        selector("transfer(Field, Field)"),
        Err(Error::InvalidSignatureText)
    );
}

#[test]
fn argument_and_calldata_hashes_match_aztec() {
    for case in payload()["argsHashes"].as_array().unwrap() {
        let args = case["args"].as_array().unwrap();
        assert_eq!(
            bare(CallKind::Private, 0, args).args_hash().unwrap(),
            base(&case["argsHash"])
        );

        // Public calldata is `[selector, ...args]`, so the first value plays the selector.
        if let Some((first, rest)) = args.split_first() {
            let selector =
                u32::from_str_radix(first.as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
            assert_eq!(
                bare(CallKind::Public, selector, rest).args_hash().unwrap(),
                base(&case["calldataHash"])
            );
        }
    }
}

#[test]
fn app_payload_and_outer_hashes_match_aztec() {
    let v = payload();
    let chain = ChainInfo {
        chain_id: base(&v["context"]["chainId"]),
        version: base(&v["context"]["version"]),
    };
    let vault = base(&v["context"]["vault"]);

    for (name, case) in v["payloads"].as_object().unwrap() {
        let calls: Vec<Call> = case["calls"].as_array().unwrap().iter().map(call).collect();
        let nonce = base(&case["txNonce"]);
        let fields: Vec<Base> = case["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(base)
            .collect();

        assert_eq!(app_payload_fields(&calls, nonce).unwrap(), fields, "{name}");
        let payload_hash = app_payload_hash(&calls, nonce).unwrap();
        assert_eq!(payload_hash, base(&case["payloadHash"]), "{name}");
        assert_eq!(
            outer_hash(vault, chain, payload_hash),
            base(&case["outerHash"]),
            "{name}"
        );
    }
}

#[test]
fn any_change_to_the_calls_or_nonce_changes_the_outer_hash() {
    let chain = ChainInfo {
        chain_id: Base::from(1u64),
        version: Base::from(1u64),
    };
    let vault = Base::from(5u64);
    let nonce = Base::from(9u64);
    let original = request(10).outer_hash(vault, chain, nonce).unwrap();
    let mut retargeted = request(10);
    retargeted.calls[0].to = field(&Base::from(0x9999u64));

    assert_ne!(
        request(11).outer_hash(vault, chain, nonce).unwrap(),
        original
    );
    assert_ne!(
        retargeted.outer_hash(vault, chain, nonce).unwrap(),
        original
    );
    assert_ne!(
        request(10)
            .outer_hash(vault, chain, Base::from(10u64))
            .unwrap(),
        original
    );
    assert_ne!(
        request(10)
            .outer_hash(Base::from(6u64), chain, nonce)
            .unwrap(),
        original
    );
}

#[test]
fn a_request_round_trips_through_its_payload_and_has_one_form() {
    let payload = request(10).to_payload().unwrap();

    assert_eq!(
        ExecutionRequest::from_payload(&payload).unwrap(),
        request(10)
    );
    let mut trailing = payload.bytes.clone();
    trailing.push(0);
    assert!(ExecutionRequest::from_payload(&ProposalPayload { bytes: trailing }).is_err());
}

#[test]
fn calls_the_entrypoint_would_mishandle_are_refused() {
    let invalid = |change: fn(&mut Call), reason| {
        let mut bad = request(10);
        change(&mut bad.calls[0]);
        assert_eq!(
            bad.to_payload().err(),
            Some(Error::InvalidCall { index: 0, reason })
        );
    };

    invalid(|c| c.to = [0; 32], "zero target");
    invalid(
        |c| c.hide_msg_sender = true,
        "hide_msg_sender on a private call",
    );
    invalid(
        |c| c.signature = Some("approve(Field)".to_owned()),
        "signature does not match the selector",
    );
    invalid(|c| c.args[0] = [0xff; 32], "argument is not a field");

    let mut crowded = request(10);
    crowded.calls = vec![crowded.calls[0].clone(); 6];
    assert_eq!(crowded.to_payload().err(), Some(Error::TooManyCalls(6)));
}

#[test]
fn arguments_split_by_the_signature_types() {
    let f = |n: u64| field(&Base::from(n));
    let transfer = arguments(
        "transfer_private_to_private((Field),(Field),u128,Field)",
        &[f(1), f(2), f(3), f(4)],
    )
    .unwrap();
    let kinds: Vec<&str> = transfer.iter().map(|a| a.kind.as_str()).collect();
    assert_eq!(kinds, ["(Field)", "(Field)", "u128", "Field"]);
    assert_eq!(transfer[2].fields, [f(3)]);

    let rotation: Vec<_> = (0..67).map(f).collect();
    let split = arguments(
        "rotate_membership([(Field,Field);8],[(Field,Field);8],u32,u32,u32,[(Field,Field);8],[(Field,Field);8])",
        &rotation,
    )
    .unwrap();
    let widths: Vec<usize> = split.iter().map(|a| a.fields.len()).collect();
    assert_eq!(widths, [16, 16, 1, 1, 1, 16, 16]);
    assert_eq!(arguments("noop()", &[]).unwrap(), []);
    assert_eq!(
        arguments("set_name(str<3>,bool)", &[f(1), f(2), f(3), f(4)]).unwrap()[0]
            .fields
            .len(),
        3
    );

    assert_eq!(
        arguments("f(Field)", &[f(1), f(2)]),
        Err(Error::UndecodableArguments(
            "more arguments than parameters"
        ))
    );
    assert_eq!(
        arguments("f(Field,Field)", &[f(1)]),
        Err(Error::UndecodableArguments(
            "fewer arguments than parameters"
        ))
    );
    assert_eq!(
        arguments("f(Mystery)", &[f(1)]),
        Err(Error::UndecodableArguments("unknown parameter type"))
    );
}
