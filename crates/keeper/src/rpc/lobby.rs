//! The `lobby.*` methods.

use ome_api::keeper as api;
use ome_wire::messages::lobby::Invite;
use zeroize::Zeroizing;

use super::convert;
use crate::{Keeper, LobbyOpen, Result};

pub(super) async fn open(keeper: &Keeper, p: api::LobbyOpenParams) -> Result<api::LobbyOpened> {
    let (lobby, invite) = keeper
        .open_lobby(LobbyOpen {
            name: p.name,
            threshold: p.threshold,
            viewing_threshold: p.viewing_threshold,
            network: p.network,
            member_name: p.member.name,
            signer_key: convert::wire_point(p.member.signer_key)?,
        })
        .await?;
    Ok(api::LobbyOpened {
        lobby: lobby.into(),
        invite,
    })
}

pub(super) async fn join(keeper: &Keeper, p: api::LobbyJoinParams) -> Result<api::LobbyJoined> {
    let signer_key = convert::wire_point(p.member.signer_key)?;
    let lobby = keeper
        .join_lobby(&p.invite, p.member.name, signer_key)
        .await?;
    Ok(api::LobbyJoined {
        lobby: lobby.into(),
    })
}

/// Decodes and validates an invite without any network traffic or state change.
pub(super) fn inspect(p: &api::InviteParams) -> Result<api::Invite> {
    Ok(convert::invite(&Invite::decode(&p.invite)?))
}

pub(super) async fn status(keeper: &Keeper, p: api::LobbyParams) -> Result<api::Lobby> {
    let info = keeper.lobby(p.lobby.0, true).await?;
    Ok(convert::lobby(&keeper.identity(), &info))
}

pub(super) async fn list(keeper: &Keeper) -> Result<Vec<api::Lobby>> {
    let me = keeper.identity();
    Ok(keeper
        .lobbies()
        .await?
        .iter()
        .map(|info| convert::lobby(&me, info))
        .collect())
}

pub(super) async fn start(keeper: &Keeper, p: api::LobbyStartParams) -> Result<api::Lobby> {
    let secrets = Zeroizing::new(p.secrets.0);
    let info = keeper
        .start_lobby(
            p.lobby.0,
            p.threshold,
            p.viewing_threshold,
            p.salt.0,
            &secrets,
        )
        .await?;
    Ok(convert::lobby(&keeper.identity(), &info))
}

pub(super) async fn close(
    keeper: &Keeper,
    p: api::LobbyParams,
    role: api::LobbyRole,
) -> Result<api::Lobby> {
    let info = keeper.close_lobby(p.lobby.0, role).await?;
    Ok(convert::lobby(&keeper.identity(), &info))
}
