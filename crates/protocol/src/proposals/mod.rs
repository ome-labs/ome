//! Spending proposals: checking a proposal against its payload, verifying votes, and tallying them.

mod approval;
mod proposal;
mod tally;

pub use approval::{approval_messages_for, verify_approval, verify_rejection};
pub use proposal::{Draft, FEE_EXTERNAL, check_proposal};
pub use tally::{Status, Tally};
