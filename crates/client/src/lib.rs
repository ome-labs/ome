//! Typed clients of an Ome member's local services, over the APIs in `ome-api`.
//!
//! - [`KeeperClient`]: the keeper, as the daemon uses it.
//! - [`DaemonClient`]: the daemon, as the CLI and apps use it; [`Home`] finds a running one.
//!
//! A client is one authenticated session. Calls take a method marker and its params and return its result; calls on
//! clones of a client share the session and run concurrently.

mod daemon;
mod discovery;
mod error;
mod keeper;
mod session;
mod transport;

pub use daemon::DaemonClient;
pub use discovery::{ADDR_FILE, Endpoint, Home, TOKEN_FILE};
pub use error::{ClientError, Result};
pub use keeper::KeeperClient;
pub use session::Events;
