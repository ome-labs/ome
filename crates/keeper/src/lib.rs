//! An Ome member's keeper: the always-on service that holds the member's viewing share for each vault and never lets
//! it out. It runs the DKG and reshare ceremonies with the other members' keepers, answers threshold ECDH requests with
//! DLEQ-proven partials and gathers them for its own member, and activates, holds or destroys shares as the vault's
//! on-chain committee changes, acting only on configs it can verify.
//!
//! A library first: [`Keeper::start`] takes its transport, clock and storage from outside, so tests run many keepers in
//! one process and an app can embed one. See `architecture/keeper.md`.

pub mod ceremony;
mod clock;
pub mod committee;
pub mod custody;
mod error;
pub mod events;
mod keeper;
mod limits;
mod random;
mod service;
pub mod store;

pub use clock::{Clock, SystemClock};
pub use error::{Error, Result};
pub use keeper::{Keeper, KeeperConfig, KeeperParts};
pub use service::{
    ceremonies::{CeremonyInfo, DkgResult, NewMember, ReshareRequest, ReshareResult},
    ecdh::MemberPartials,
    vaults::{Observation, VaultInfo},
};
