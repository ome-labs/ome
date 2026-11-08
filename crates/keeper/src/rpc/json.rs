//! JSON shapes of the local RPC. 32-byte values are `0x`-prefixed hex; points are `{ x, y }` (the TypeScript
//! convention); message bodies are hex of their canonical wire encoding.

use ome_crypto::threshold::ecdh::Partial;
use ome_wire::{
    IdentityPublic,
    messages::ceremony::{Manifest, Member},
    types::{WirePoint, scalar},
};
use serde::{Deserialize, Serialize};

use crate::{
    DkgResult, Error, MemberPartials, NewMember, ReshareResult, Result,
    ceremony::{Failure, Participant},
};

pub(crate) fn hex32(bytes: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}

pub(crate) fn parse32(value: &str) -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    hex::decode_to_slice(value.strip_prefix("0x").unwrap_or(value), &mut out)
        .map_err(|_| Error::Invalid(format!("expected 32 bytes of hex, got {value:?}")))?;
    Ok(out)
}

pub(crate) fn hex16(bytes: &[u8; 16]) -> String {
    format!("0x{}", hex::encode(bytes))
}

pub(crate) fn parse16(value: &str) -> Result<[u8; 16]> {
    let mut out = [0u8; 16];
    hex::decode_to_slice(value.strip_prefix("0x").unwrap_or(value), &mut out)
        .map_err(|_| Error::Invalid(format!("expected 16 bytes of hex, got {value:?}")))?;
    Ok(out)
}

pub(crate) fn parse_bytes(value: &str) -> Result<Vec<u8>> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value))
        .map_err(|_| Error::Invalid("expected hex".into()))
}

/// A point.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct JsonPoint {
    /// `x`.
    pub(crate) x: String,
    /// `y`.
    pub(crate) y: String,
}

impl From<&WirePoint> for JsonPoint {
    fn from(point: &WirePoint) -> Self {
        Self {
            x: hex32(&point.x),
            y: hex32(&point.y),
        }
    }
}

impl JsonPoint {
    pub(crate) fn to_wire(&self) -> Result<WirePoint> {
        let point = WirePoint {
            x: parse32(&self.x)?,
            y: parse32(&self.y)?,
        };
        ome_crypto::Point::try_from(&point)?;
        Ok(point)
    }
}

/// A keeper to talk to.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonParticipant {
    /// Identity (hex).
    pub(crate) identity: String,
    /// Transport key (hex).
    pub(crate) transport: String,
    /// Addresses.
    pub(crate) addresses: Vec<String>,
}

impl JsonParticipant {
    pub(crate) fn to_participant(&self) -> Result<Participant> {
        Ok(Participant {
            identity: IdentityPublic(parse32(&self.identity)?),
            transport: parse32(&self.transport)?,
            addresses: self.addresses.clone(),
        })
    }
}

/// A manifest member.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonMember {
    /// 1-based index.
    pub(crate) index: u32,
    /// Signing key.
    pub(crate) signer_key: JsonPoint,
    /// Verification share.
    pub(crate) verification_share: JsonPoint,
    /// Keeper identity (hex).
    pub(crate) identity: String,
    /// Keeper transport key (hex).
    pub(crate) transport: String,
    /// Keeper addresses.
    pub(crate) addresses: Vec<String>,
}

/// A committee manifest.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonManifest {
    /// Protocol version the vault's members agreed on.
    #[serde(default)]
    pub(crate) protocol_version: u16,
    /// Vault address.
    pub(crate) vault: String,
    /// Committee epoch.
    pub(crate) epoch: u32,
    /// Spending threshold `k`.
    pub(crate) threshold: u32,
    /// Viewing threshold `t_v`.
    pub(crate) viewing_threshold: u32,
    /// Preaddress.
    pub(crate) preaddress: String,
    /// Group key `Ivpk`.
    pub(crate) group_key: JsonPoint,
    /// Members in slot order.
    pub(crate) members: Vec<JsonMember>,
}

impl From<&Manifest> for JsonManifest {
    fn from(m: &Manifest) -> Self {
        Self {
            protocol_version: m.protocol_version,
            vault: hex32(&m.vault),
            epoch: m.epoch,
            threshold: m.threshold,
            viewing_threshold: m.viewing_threshold,
            preaddress: hex32(&m.preaddress),
            group_key: JsonPoint::from(&m.group_key),
            members: m
                .members
                .iter()
                .map(|member| JsonMember {
                    index: member.index,
                    signer_key: JsonPoint::from(&member.signer_key),
                    verification_share: JsonPoint::from(&member.verification_share),
                    identity: hex32(&member.identity.0),
                    transport: hex32(&member.transport),
                    addresses: member.addresses.clone(),
                })
                .collect(),
        }
    }
}

impl JsonManifest {
    pub(crate) fn to_manifest(&self) -> Result<Manifest> {
        Ok(Manifest {
            protocol_version: self.protocol_version,
            vault: parse32(&self.vault)?,
            epoch: self.epoch,
            threshold: self.threshold,
            viewing_threshold: self.viewing_threshold,
            preaddress: parse32(&self.preaddress)?,
            group_key: self.group_key.to_wire()?,
            members: self
                .members
                .iter()
                .map(|m| {
                    Ok(Member {
                        index: m.index,
                        signer_key: m.signer_key.to_wire()?,
                        verification_share: m.verification_share.to_wire()?,
                        identity: IdentityPublic(parse32(&m.identity)?),
                        transport: parse32(&m.transport)?,
                        addresses: m.addresses.clone(),
                    })
                })
                .collect::<Result<_>>()?,
        })
    }
}

/// A member of a reshare's new committee.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonNewMember {
    /// Keeper identity (hex).
    pub(crate) identity: String,
    /// Keeper transport key (hex).
    pub(crate) transport: String,
    /// Keeper addresses.
    pub(crate) addresses: Vec<String>,
    /// Signing key.
    pub(crate) signer_key: JsonPoint,
}

impl JsonNewMember {
    pub(crate) fn to_new_member(&self) -> Result<NewMember> {
        Ok(NewMember {
            identity: IdentityPublic(parse32(&self.identity)?),
            transport: parse32(&self.transport)?,
            addresses: self.addresses.clone(),
            signer_key: self.signer_key.to_wire()?,
        })
    }
}

/// One partial (the `@ome/napi` `EcdhPartial` shape).
#[derive(Clone, Debug, Serialize)]
pub(crate) struct JsonPartial {
    /// `D_i`.
    pub(crate) point: JsonPoint,
    /// DLEQ challenge.
    pub(crate) challenge: String,
    /// DLEQ response.
    pub(crate) response: String,
}

/// A member's partials (the `@ome/napi` `MemberPartials` shape).
#[derive(Clone, Debug, Serialize)]
pub(crate) struct JsonMemberPartials {
    /// Member index.
    pub(crate) member: u32,
    /// Partials in input order.
    pub(crate) partials: Vec<JsonPartial>,
}

impl From<&MemberPartials> for JsonMemberPartials {
    fn from(m: &MemberPartials) -> Self {
        Self {
            member: m.member,
            partials: m.partials.iter().map(json_partial).collect(),
        }
    }
}

fn json_partial(p: &Partial) -> JsonPartial {
    JsonPartial {
        point: JsonPoint::from(&WirePoint::from(&p.point)),
        challenge: hex32(&scalar(&p.proof.challenge)),
        response: hex32(&scalar(&p.proof.response)),
    }
}

/// A finished DKG.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonDkgResult {
    /// This keeper's index.
    pub(crate) index: u32,
    /// Viewing threshold.
    pub(crate) threshold: u32,
    /// Group key.
    pub(crate) group_key: JsonPoint,
    /// Verification shares in member order.
    pub(crate) verification_shares: Vec<JsonPoint>,
    /// Transcript hash (hex).
    pub(crate) transcript: String,
}

impl From<&DkgResult> for JsonDkgResult {
    fn from(r: &DkgResult) -> Self {
        Self {
            index: r.index,
            threshold: r.threshold,
            group_key: JsonPoint::from(&r.group_key),
            verification_shares: r.verification_shares.iter().map(JsonPoint::from).collect(),
            transcript: format!("0x{}", hex::encode(r.transcript.to_array())),
        }
    }
}

/// A finished reshare.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonReshareResult {
    /// The new committee's commitment, for `rotate_membership` (hex).
    pub(crate) committee_commitment: String,
    /// This keeper's index in the new committee.
    pub(crate) new_index: Option<u32>,
    /// The new manifest (epoch set at activation).
    pub(crate) manifest: Option<JsonManifest>,
    /// Transcript hash (hex).
    pub(crate) transcript: Option<String>,
}

impl From<&ReshareResult> for JsonReshareResult {
    fn from(r: &ReshareResult) -> Self {
        Self {
            committee_commitment: hex32(&r.committee_commitment),
            new_index: r.new_index,
            manifest: r.manifest.as_ref().map(JsonManifest::from),
            transcript: r
                .transcript
                .map(|t| format!("0x{}", hex::encode(t.to_array()))),
        }
    }
}

/// Ceremony status.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct JsonCeremony {
    /// `dkg` or `reshare`.
    pub(crate) kind: String,
    /// Status.
    pub(crate) status: String,
    /// Failure, if aborted.
    pub(crate) failure: Option<Failure>,
    /// Result, if completed.
    pub(crate) result: Option<serde_json::Value>,
}
