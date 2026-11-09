//! Local JSON-RPC 2.0 for the member's own daemon (`ome_api::keeper`): newline-delimited JSON over a Unix domain socket
//! (named pipe on Windows), owner-only, with a per-launch token presented through `session.auth` before anything else;
//! and, for a keeper on the member's own server, over TLS with a pinned self-signed certificate.

mod convert;
mod devices;
mod lobby;
mod methods;
mod server;
mod tls;

pub use server::{RpcToken, serve};
pub use tls::{TlsIdentity, serve_tls};
