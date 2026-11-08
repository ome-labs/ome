//! JSON shapes and handlers of the `lobby.*` methods.

use ome_wire::messages::lobby::{Invite, KeeperContact};
use serde::Deserialize;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::json::{JsonPoint, hex16, hex32, parse_bytes, parse16, parse32};
use crate::{Error, Keeper, LobbyInfo, LobbyOpen, LobbyRole, Result};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemberParam {
    name: String,
    signer_key: JsonPoint,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Open {
    name: String,
    threshold: u8,
    viewing_threshold: u8,
    network: String,
    member: MemberParam,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Join {
    invite: String,
    member: MemberParam,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InviteParam {
    invite: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LobbyParam {
    lobby: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Start {
    lobby: String,
    threshold: u8,
    viewing_threshold: u8,
    salt: String,
    secrets: String,
}

fn params<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::Invalid(format!("params: {e}")))
}

fn lobby_param(value: Value) -> Result<[u8; 16]> {
    parse16(&params::<LobbyParam>(value)?.lobby)
}

fn contact_json(contact: &KeeperContact) -> Value {
    json!({
        "identity": hex32(&contact.identity.0),
        "transport": hex32(&contact.transport),
        "addresses": contact.addresses,
    })
}

fn info_json(keeper: &Keeper, info: &LobbyInfo) -> Value {
    let me = keeper.identity();
    let members: Vec<Value> = info
        .members
        .iter()
        .zip(1u32..)
        .map(|(m, index)| {
            json!({
                "index": index,
                "name": m.name,
                "signerKey": JsonPoint::from(&m.signer_key),
                "identity": hex32(&m.keeper.identity.0),
                "transport": hex32(&m.keeper.transport),
                "addresses": m.keeper.addresses,
                "you": m.keeper.identity == me,
            })
        })
        .collect();
    let mut value = json!({
        "lobby": hex16(&info.lobby),
        "role": match info.role {
            LobbyRole::Host => "host",
            LobbyRole::Guest => "guest",
        },
        "status": info.status.as_str(),
        "name": info.name,
        "hostName": info.host_name,
        "threshold": info.threshold,
        "viewingThreshold": info.viewing_threshold,
        "network": info.network,
        "host": contact_json(&info.host),
        "members": members,
    });
    if let Some(plan) = &info.plan {
        let mut plan_json = json!({
            "session": hex32(&plan.session),
            "threshold": plan.threshold,
            "viewingThreshold": plan.viewing_threshold,
            "salt": hex32(&plan.salt),
        });
        if let Some(secrets) = &plan.secrets {
            plan_json["secrets"] = Value::String(format!("0x{}", hex::encode(secrets.as_slice())));
        }
        value["plan"] = plan_json;
    }
    if let Some(invite) = &info.invite {
        value["invite"] = Value::String(invite.clone());
    }
    value
}

fn inspect_json(invite: &Invite) -> Value {
    json!({
        "lobby": hex16(&invite.lobby),
        "name": invite.name,
        "hostName": invite.host_name,
        "threshold": invite.threshold,
        "viewingThreshold": invite.viewing_threshold,
        "network": invite.network,
        "host": contact_json(&invite.host),
    })
}

/// Runs a `lobby.*` method.
pub(crate) async fn call(keeper: &Keeper, method: &str, value: Value) -> Result<Value> {
    match method {
        "lobby.open" => open(keeper, value).await,
        "lobby.join" => join(keeper, value).await,
        "lobby.inspect" => inspect(value),
        "lobby.status" => status(keeper, value).await,
        "lobby.list" => keeper
            .lobbies()
            .await
            .map(|all| Value::Array(all.iter().map(|i| info_json(keeper, i)).collect())),
        "lobby.start" => start(keeper, value).await,
        "lobby.close" => close(keeper, value, LobbyRole::Host).await,
        "lobby.leave" => close(keeper, value, LobbyRole::Guest).await,
        _ => Err(Error::Invalid(format!("unknown method {method}"))),
    }
}

/// Decodes and validates an invite without any network traffic or state change.
fn inspect(value: Value) -> Result<Value> {
    let p: InviteParam = params(value)?;
    Ok(inspect_json(&Invite::decode(&p.invite)?))
}

async fn status(keeper: &Keeper, value: Value) -> Result<Value> {
    let info = keeper.lobby(lobby_param(value)?, true).await?;
    Ok(info_json(keeper, &info))
}

async fn open(keeper: &Keeper, value: Value) -> Result<Value> {
    let p: Open = params(value)?;
    let (lobby, invite) = keeper
        .open_lobby(LobbyOpen {
            name: p.name,
            threshold: p.threshold,
            viewing_threshold: p.viewing_threshold,
            network: p.network,
            member_name: p.member.name,
            signer_key: p.member.signer_key.to_wire()?,
        })
        .await?;
    Ok(json!({ "lobby": hex16(&lobby), "invite": invite }))
}

async fn join(keeper: &Keeper, value: Value) -> Result<Value> {
    let p: Join = params(value)?;
    let lobby = keeper
        .join_lobby(&p.invite, p.member.name, p.member.signer_key.to_wire()?)
        .await?;
    Ok(json!({ "lobby": hex16(&lobby) }))
}

async fn start(keeper: &Keeper, value: Value) -> Result<Value> {
    let p: Start = params(value)?;
    let secrets = Zeroizing::new(parse_bytes(&p.secrets)?);
    let info = keeper
        .start_lobby(
            parse16(&p.lobby)?,
            p.threshold,
            p.viewing_threshold,
            parse32(&p.salt)?,
            &secrets,
        )
        .await?;
    Ok(info_json(keeper, &info))
}

async fn close(keeper: &Keeper, value: Value, role: LobbyRole) -> Result<Value> {
    let info = keeper.close_lobby(lobby_param(value)?, role).await?;
    Ok(info_json(keeper, &info))
}
