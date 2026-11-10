//! A client reads values a newer release sends: an intent kind, a proposal status, a simulation failure or an event
//! it doesn't know becomes the enum's unknown case instead of failing to parse.
#![allow(clippy::unwrap_used)]

use ome_api::daemon::{DaemonEvent, Intent, ProposalStatus, SimulationFailure};

#[test]
fn unknown_values_from_a_newer_release_parse() {
    let intent: Intent =
        serde_json::from_str(r#"{"kind":"batch","payments":[1,2],"memo":null}"#).unwrap();
    assert_eq!(intent, Intent::Unknown);
    let status: ProposalStatus = serde_json::from_str(r#""scheduled""#).unwrap();
    assert_eq!(status, ProposalStatus::Unknown);
    let failure: SimulationFailure =
        serde_json::from_str(r#"{"kind":"policyLimit","limit":"5"}"#).unwrap();
    assert_eq!(failure, SimulationFailure::Unknown);
    let event: DaemonEvent = serde_json::from_str(r#"{"type":"batchProgress","done":3}"#).unwrap();
    assert_eq!(event, DaemonEvent::Unknown);

    // Known values still parse as themselves.
    let known: Intent =
        serde_json::from_str(r#"{"kind":"invalidatePending","memo":null}"#).unwrap();
    assert_eq!(known, Intent::InvalidatePending { memo: None });
}
