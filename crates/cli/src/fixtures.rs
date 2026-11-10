//! Values for renderer tests.

use ome_api::{
    Amount, Hex32,
    daemon::{
        AbiType, AbiValue, BalanceChange, DecodedArg, DecodedCall, Effects, FeeEstimate,
        FunctionKind, Gas, Intent, KnownType, LabelSource, NoteCounts, ProposalStatus,
        ProposalView, Proposer, Recipient, Review, ReviewChecks, Signs, Simulation, TokenInfo,
        Visibility, Vote,
    },
};

use crate::ui::format;

pub(crate) fn hex(prefix: &str, tail: &str) -> Hex32 {
    format!("0x{prefix}{tail:0>width$}", width = 64 - prefix.len())
        .parse()
        .unwrap()
}

pub(crate) fn usdc() -> TokenInfo {
    TokenInfo {
        address: hex("05dc", "a1"),
        name: "USD Coin".into(),
        symbol: "USDC".into(),
        decimals: 6,
        standard: true,
    }
}

pub(crate) fn proposal() -> ProposalView {
    let bob = hex("2b1f", "9e04");
    ProposalView {
        id: hex("4f2ac91e", "77"),
        vault: hex("7a11", "01"),
        intent: Intent::Transfer {
            token: usdc().address,
            to: bob,
            amount: Amount(250_000_000),
            memo: Some("March rent".into()),
        },
        proposer: Proposer {
            index: 1,
            name: "Alice".into(),
        },
        status: ProposalStatus::Collecting,
        threshold: 2,
        members: 3,
        votes: vec![Vote {
            slot: 0,
            index: 1,
            name: "Alice".into(),
            approve: true,
            valid: true,
            reason: None,
            at: format::now() - 2 * 3600,
        }],
        my_slot: 1,
        approved_by_me: false,
        rejected_by_me: false,
        config_version: 4,
        current_config_version: 4,
        // Five hours and twelve minutes from now, with slack for a slow test.
        created_at: format::now() - 2 * 3600,
        expires_at: format::now() + 5 * 3600 + 12 * 60 + 30,
        tx_nonce: hex("", "7"),
        outer_hash: hex("91c3", "e2a0"),
        approval_message: hex("0e7a", "44b1"),
        fee_method: "the sponsored fee payer 0x2d8f…69d3".into(),
        calls: vec![DecodedCall {
            to: usdc().address,
            to_label: Some("USDC".into()),
            function: "transfer_in_private".into(),
            selector: "0x7a3f21c0".into(),
            kind: FunctionKind::Private,
            args: vec![
                arg(
                    "from",
                    address(),
                    &hex("7a11", "01").to_string(),
                    Some("Treasury (this vault)"),
                ),
                arg("to", address(), &bob.to_string(), Some("bob (alias)")),
                arg(
                    "amount",
                    AbiType::Integer {
                        signed: false,
                        width: 128,
                    },
                    "250000000",
                    Some("250 USDC"),
                ),
                arg("authwit_nonce", AbiType::Field, "0x07", None),
            ],
            is_static: false,
            decoded: true,
        }],
        token: Some(usdc()),
        execution: None,
        executing: None,
        cancelled_by: None,
    }
}

pub(crate) fn review() -> Review {
    let proposal = proposal();
    Review {
        vault_name: "Treasury".into(),
        my_name: "Bob".into(),
        checks: ReviewChecks {
            rebuilt: true,
            outer_hash: proposal.outer_hash,
            config_current: true,
            not_expired: true,
            failure: None,
        },
        action: "transfer 250 USDC (250000000 base units)".into(),
        recipient: Some(Recipient {
            address: hex("2b1f", "9e04"),
            label: Some("bob".into()),
            source: Some(LabelSource::AztecWallet),
        }),
        simulation: Simulation {
            ok: true,
            gas: Some(Gas {
                da: 1024,
                l2: 12_840,
            }),
            error: None,
            effects: Some(Effects {
                balances: vec![BalanceChange {
                    token: usdc(),
                    before: Amount(1_000_000_000),
                    after: Amount(750_000_000),
                    visibility: Visibility::Private,
                }],
                returns: Vec::new(),
                notes: NoteCounts {
                    created: 2,
                    nullified: 2,
                },
                complete: true,
            }),
            fee: Some(FeeEstimate {
                gas: Gas {
                    da: 1024,
                    l2: 12_840,
                },
                fee: Amount(1_200_000_000_000_000),
                max_fee: Amount(1_800_000_000_000_000),
                payer: hex("2d8f", "69d3"),
            }),
        },
        change: None,
        signs: Signs {
            message: proposal.approval_message,
            formula: "Poseidon2(\"ome.approval.v0\", [vault, outer hash, 4, deadline])".into(),
        },
        rejects: Signs {
            message: hex("5e1e", "c7"),
            formula: "Poseidon2(\"ome.rejection.v0\", [vault, outer hash, 4, deadline])".into(),
        },
        proposal,
    }
}

fn address() -> AbiType {
    AbiType::Struct {
        path: "aztec::protocol_types::address::aztec_address::AztecAddress".into(),
        fields: vec![],
        known: Some(KnownType::AztecAddress),
    }
}

fn arg(name: &str, ty: AbiType, value: &str, display: Option<&str>) -> DecodedArg {
    DecodedArg {
        name: name.into(),
        ty,
        value: AbiValue::Text(value.into()),
        display: display.map(Into::into),
    }
}
