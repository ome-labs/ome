//! Conversions between the keeper's types and the ome-api shapes it serves. Points are checked to be on the curve on
//! the way in.

use ome_api::{
    Point,
    keeper::{self as api, VaultStatus},
};
use ome_crypto::threshold::ecdh::Partial;
use ome_net::identity::peer_id;
use ome_wire::{
    IdentityPublic, Message, decode_body,
    messages::{
        lobby::{Invite, KeeperContact},
        proposals::ExecutionReceipt,
    },
    types::{Bytes64, scalar},
};

use crate::{
    DkgResult, Error, LobbyInfo, MemberPartials, MemberReachability, NewMember, ReshareResult,
    Result, VaultInfo,
    ceremony::{Failure, Participant},
    service::ceremonies::{ceremony_kind, ceremony_status},
    store::{AuditEntry, ReceiptEntry},
};

fn transcript(bytes: &Bytes64) -> ome_api::Hex64 {
    ome_api::Hex64(bytes.to_array())
}

pub(super) fn identity_peer_id(identity: &IdentityPublic) -> String {
    peer_id(identity)
        .map(|id| id.to_string())
        .unwrap_or_default()
}

pub(super) fn participant(contact: api::KeeperContact) -> Participant {
    Participant {
        identity: IdentityPublic(contact.identity.0),
        transport: contact.transport.0,
        addresses: contact.addresses,
    }
}

pub(super) fn new_member(member: api::NewMember) -> Result<NewMember> {
    Ok(NewMember {
        identity: IdentityPublic(member.identity.0),
        transport: member.transport.0,
        addresses: member.addresses,
        signer_key: member.signer_key.try_into()?,
    })
}

pub(super) fn contact(contact: &KeeperContact) -> api::KeeperContact {
    api::KeeperContact {
        identity: contact.identity.0.into(),
        transport: contact.transport.into(),
        addresses: contact.addresses.clone(),
    }
}

pub(super) fn vault(info: &VaultInfo) -> api::Vault {
    api::Vault {
        manifest: api::Manifest::from(&info.manifest),
        status: match info.status {
            "retired" => VaultStatus::Retired,
            "removed" => VaultStatus::Removed,
            _ => VaultStatus::Active,
        },
        index: info.index,
        serving: info.serving,
    }
}

pub(super) fn reachability(member: &MemberReachability) -> api::Reachability {
    api::Reachability {
        index: member.index,
        identity: member.identity.0.into(),
        peer_id: identity_peer_id(&member.identity),
        you: member.you,
        connected: member.status.connected,
        relayed: member.status.relayed,
        last_seen: member.status.last_seen,
        rtt_ms: member.status.rtt_ms,
    }
}

/// The signed receipt in a stored entry, when it has one.
pub(super) fn signed_receipt(entry: &ReceiptEntry) -> Option<ExecutionReceipt> {
    match decode_body(entry.body.as_deref()?) {
        Ok(Message::ExecutionReceipt(receipt)) => Some(receipt),
        _ => None,
    }
}

pub(super) fn receipt(
    entry: &ReceiptEntry,
    signed: Option<&ExecutionReceipt>,
    executor: Option<u32>,
) -> api::Receipt {
    api::Receipt {
        proposal: entry.proposal.into(),
        tx_hash: entry.tx_hash.into(),
        at: entry.at,
        block: signed.map_or(0, |r| r.block),
        executor,
    }
}

pub(super) fn votes(entries: Vec<(Vec<u8>, u64)>) -> Vec<api::StoredVote> {
    entries
        .into_iter()
        .map(|(body, at)| api::StoredVote {
            body: body.into(),
            at,
        })
        .collect()
}

pub(super) fn audit(entry: AuditEntry) -> api::AuditEntry {
    api::AuditEntry {
        at: entry.at,
        requester: entry.requester,
        purpose: entry.purpose,
        points: entry.points,
        outcome: entry.outcome,
    }
}

fn partial(p: &Partial) -> api::Partial {
    api::Partial {
        point: Point::from_core(&p.point),
        challenge: scalar(&p.proof.challenge).into(),
        response: scalar(&p.proof.response).into(),
    }
}

pub(super) fn member_partials(m: &MemberPartials) -> api::MemberPartials {
    api::MemberPartials {
        member: m.member,
        partials: m.partials.iter().map(partial).collect(),
    }
}

fn failure(failure: Failure) -> api::Failure {
    api::Failure {
        reason: failure.reason,
        blamed: failure.blamed,
    }
}

fn dkg_result(r: &DkgResult) -> api::DkgResult {
    api::DkgResult {
        index: r.index,
        threshold: r.threshold,
        group_key: Point::from(&r.group_key),
        verification_shares: r.verification_shares.iter().map(Point::from).collect(),
        transcript: transcript(&r.transcript),
    }
}

fn reshare_result(r: &ReshareResult, secrets: Option<&[u8]>) -> api::ReshareResult {
    api::ReshareResult {
        committee_commitment: r.committee_commitment.into(),
        new_index: r.new_index,
        manifest: r.manifest.as_ref().map(api::Manifest::from),
        transcript: r.transcript.as_ref().map(transcript),
        secrets: secrets.map(|s| s.to_vec().into()),
    }
}

/// A ceremony's status; `secrets` are the protocol secrets this keeper received as a joiner of a reshare.
pub(super) fn ceremony(info: crate::CeremonyInfo, secrets: Option<&[u8]>) -> Result<api::Ceremony> {
    let kind = ceremony_kind(&info.kind);
    let stored = |e: serde_json::Error| Error::Storage(format!("ceremony result: {e}"));
    let result = match (kind, info.result) {
        (_, None) => None,
        (api::CeremonyKind::Dkg, Some(value)) => Some(api::CeremonyResult::Dkg(dkg_result(
            &serde_json::from_value(value).map_err(stored)?,
        ))),
        (api::CeremonyKind::Reshare, Some(value)) => Some(api::CeremonyResult::Reshare(
            reshare_result(&serde_json::from_value(value).map_err(stored)?, secrets),
        )),
    };
    Ok(api::Ceremony {
        kind,
        status: ceremony_status(&info.status),
        failure: info.failure.map(failure),
        result,
    })
}

pub(super) fn invite(invite: &Invite) -> api::Invite {
    api::Invite {
        lobby: invite.lobby.into(),
        name: invite.name.clone(),
        host_name: invite.host_name.clone(),
        threshold: invite.threshold,
        viewing_threshold: invite.viewing_threshold,
        network: invite.network.clone(),
        host: contact(&invite.host),
    }
}

pub(super) fn lobby(me: &IdentityPublic, info: &LobbyInfo) -> api::Lobby {
    api::Lobby {
        lobby: info.lobby.into(),
        role: info.role,
        status: info.status,
        name: info.name.clone(),
        host_name: info.host_name.clone(),
        threshold: info.threshold,
        viewing_threshold: info.viewing_threshold,
        network: info.network.clone(),
        host: contact(&info.host),
        members: info
            .members
            .iter()
            .zip(1u32..)
            .map(|(m, index)| api::LobbyMember {
                index,
                name: m.name.clone(),
                signer_key: Point::from(&m.signer_key),
                identity: m.keeper.identity.0.into(),
                transport: m.keeper.transport.into(),
                addresses: m.keeper.addresses.clone(),
                you: m.keeper.identity == *me,
            })
            .collect(),
        plan: info.plan.as_ref().map(|plan| api::LobbyPlan {
            session: plan.session.into(),
            threshold: plan.threshold,
            viewing_threshold: plan.viewing_threshold,
            salt: plan.salt.into(),
            secrets: plan
                .secrets
                .as_ref()
                .map(|secrets| secrets.as_slice().to_vec().into()),
            protocol_version: plan.protocol_version,
        }),
        invite: info.invite.clone(),
    }
}

#[allow(clippy::unnecessary_wraps)] // Every method body returns a `Result`.
pub(super) fn artifact_status(
    progress: crate::service::artifacts::ArtifactProgress,
) -> Result<api::ArtifactStatus> {
    Ok(api::ArtifactStatus {
        complete: progress.complete,
        have: progress.have,
        count: progress.count,
    })
}
