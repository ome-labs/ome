//! Relay counters exported as Prometheus text. Aggregates only: no peer id, mailbox id or envelope id is ever
//! recorded, so the metrics say how busy the relay is and nothing about who uses it.

use std::{
    fmt::Write as _,
    sync::atomic::{AtomicU64, Ordering},
};

use ome_wire::messages::mailbox::Refusal;

use crate::store::Usage;

/// Relay counters.
#[derive(Debug, Default)]
pub struct Metrics {
    /// Envelopes stored.
    pub deposits: AtomicU64,
    /// Requests refused, by reason: too large, bad expiry, quota, not allowed, malformed.
    pub refused: [AtomicU64; 5],
    /// Fetch requests.
    pub fetches: AtomicU64,
    /// Envelopes returned by fetches.
    pub delivered: AtomicU64,
    /// Envelopes deleted by their recipients.
    pub deleted: AtomicU64,
    /// Circuit reservations accepted.
    pub reservations: AtomicU64,
    /// Circuits relayed.
    pub circuits: AtomicU64,
    /// Circuits open now.
    pub active_circuits: AtomicU64,
}

const REASONS: [(Refusal, &str); 5] = [
    (Refusal::TooLarge, "too_large"),
    (Refusal::BadExpiry, "bad_expiry"),
    (Refusal::QuotaExceeded, "quota"),
    (Refusal::NotAllowed, "not_allowed"),
    (Refusal::Malformed, "malformed"),
];

impl Metrics {
    pub(crate) fn add(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
    }

    /// Counts a refusal under its reason.
    pub(crate) fn refused(&self, reason: Refusal) {
        if let Some(i) = REASONS.iter().position(|(r, _)| *r == reason) {
            Self::add(&self.refused[i], 1);
        }
    }

    pub(crate) fn circuit_closed(&self) {
        let _ = self
            .active_circuits
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1));
    }

    /// Prometheus text exposition.
    #[must_use]
    pub fn render(&self, usage: Usage) -> String {
        let mut out = String::new();
        let counters = [
            ("deposits_total", &self.deposits),
            ("fetches_total", &self.fetches),
            ("envelopes_delivered_total", &self.delivered),
            ("envelopes_deleted_total", &self.deleted),
            ("reservations_total", &self.reservations),
            ("circuits_total", &self.circuits),
        ];
        for (name, value) in counters {
            let _ = writeln!(
                out,
                "# TYPE ome_relay_{name} counter\nome_relay_{name} {}",
                value.load(Ordering::Relaxed)
            );
        }
        let _ = writeln!(out, "# TYPE ome_relay_refused_total counter");
        for ((_, label), value) in REASONS.iter().zip(&self.refused) {
            let _ = writeln!(
                out,
                "ome_relay_refused_total{{reason=\"{label}\"}} {}",
                value.load(Ordering::Relaxed)
            );
        }
        let gauges = [
            (
                "circuits_active",
                self.active_circuits.load(Ordering::Relaxed),
            ),
            ("envelopes", usage.envelopes),
            ("bytes", usage.bytes),
        ];
        for (name, value) in gauges {
            let _ = writeln!(
                out,
                "# TYPE ome_relay_{name} gauge\nome_relay_{name} {value}"
            );
        }
        out
    }
}
