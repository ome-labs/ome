//! The hash a vault transaction is authorized under, recomputed from its calls.
//!
//! Aztec's account entrypoint pads the calls to five, encodes each as six fields and hashes them with the transaction
//! nonce into the app payload hash. The vault then authorizes the authwit outer hash of that value, bound to the vault,
//! the chain and the rollup version. A member recomputes both from the payload before signing, so a proposer cannot
//! show one transaction and collect approvals for another. Golden-tested against `@aztec-labs/entrypoints` 6.0.0.

mod app_payload;
mod call;
mod outer_hash;
mod request;
mod selector;

pub use app_payload::{app_payload_fields, app_payload_hash};
pub use call::{Call, CallKind};
pub use outer_hash::{ChainInfo, outer_hash};
pub use request::ExecutionRequest;
pub use selector::selector;
