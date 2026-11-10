//! Every keeper method except `session.auth` and `events.subscribe`, which the server handles: params and results are
//! the ome-api types, and a keeper error becomes its API error kind.

use std::future::Future;

use ome_api::{
    API_VERSION, ErrorKind, Method, RpcError,
    jsonrpc::Request,
    keeper::{
        self as api, ApprovalsList, ApprovalsPut, ArtifactsFetch, ArtifactsGet, ArtifactsPut,
        ArtifactsStatus, AuditList, CeremonyDkg, CeremonyReshare, CeremonyStatusGet, DevicesList,
        DevicesOffer, DevicesReview, DevicesRevoke, EcdhBatch, KeeperInfoGet, KeeperJoinRequest,
        LobbyClose, LobbyInspect, LobbyJoin, LobbyLeave, LobbyList, LobbyOpen, LobbyStart,
        LobbyStatusGet, MailboxPoll, PayloadsGet, PayloadsPut, PeersStatus, ProposalsGet,
        ProposalsList, ProposalsPut, ReceiptsList, ReceiptsPut, RejectionsList, RejectionsPut,
        VaultList, VaultObserveConfig, VaultObserveTx, VaultRegister, VaultRetire, VaultStatusGet,
        VaultSync, VaultUpdateSigners,
    },
};
use ome_protocol::vault::Config;
use ome_wire::messages::ecdh::EcdhPurpose;
use serde_json::Value;
use zeroize::Zeroizing;

use super::{convert, devices, lobby};
use crate::{Keeper, ReshareRequest, Result, ceremony::dkg::DkgParams};

/// Most audit entries one call returns.
const MAX_AUDIT: u32 = 10_000;

/// Reads `M`'s params, runs `handler` and serializes its result.
async fn run<M, H, F>(request: &Request, handler: H) -> Result<Value, RpcError>
where
    M: Method,
    H: FnOnce(M::Params) -> F,
    F: Future<Output = Result<M::Result>>,
{
    let params = request
        .params::<M::Params>()
        .map_err(|e| RpcError::new(ErrorKind::Invalid, format!("params: {e}")))?;
    let result = handler(params).await?;
    serde_json::to_value(result).map_err(|e| RpcError::new(ErrorKind::Failed, e.to_string()))
}

macro_rules! route {
    ($request:ident; $($marker:ident => $handler:expr;)*) => {
        match $request.method.as_str() {
            $(<$marker as Method>::NAME => run::<$marker, _, _>($request, $handler).await,)*
            other => Err(RpcError::new(ErrorKind::MethodNotFound, format!("unknown method {other}"))),
        }
    };
}

fn hex_bodies(bodies: Vec<Vec<u8>>) -> Vec<ome_api::HexBytes> {
    bodies.into_iter().map(Into::into).collect()
}

const fn purpose(purpose: api::EcdhPurpose) -> EcdhPurpose {
    match purpose {
        api::EcdhPurpose::Decryption => EcdhPurpose::NoteDecryption,
        api::EcdhPurpose::Tagging => EcdhPurpose::RecipientTag,
        api::EcdhPurpose::SenderTag => EcdhPurpose::SenderTag,
        api::EcdhPurpose::Handshake => EcdhPurpose::Handshake,
    }
}

fn info(keeper: &Keeper) -> api::KeeperInfo {
    let identity = keeper.identity();
    api::KeeperInfo {
        api_version: API_VERSION,
        version: ome_api::RELEASE_VERSION.into(),
        identity: identity.0.into(),
        transport: keeper.transport_key().into(),
        peer_id: convert::identity_peer_id(&identity),
        addresses: keeper.addresses().to_vec(),
    }
}

/// Runs one method.
#[expect(clippy::too_many_lines, reason = "one route per method")]
pub(super) async fn call(keeper: &Keeper, request: &Request) -> Result<Value, RpcError> {
    route! { request;
        KeeperInfoGet => |_| async { Ok(info(keeper)) };
        KeeperJoinRequest => |p: api::LobbyMemberParams| async move {
            let request = keeper.join_request(p.name, p.signer_key.try_into()?)?;
            Ok(api::JoinRequestText { request })
        };
        VaultList => |_| async { Ok(keeper.vaults().await.iter().map(convert::vault).collect()) };
        VaultStatusGet => |p: api::VaultParams| async move { Ok(convert::vault(&keeper.vault(p.vault.0).await?)) };
        VaultRegister => |p: api::RegisterParams| async move {
            let manifest = p.manifest.try_into()?;
            Ok(convert::vault(&keeper.register_vault(p.session.0, manifest).await?))
        };
        VaultObserveConfig => |p: api::ObserveConfigParams| async move {
            let observation = keeper.observe_config(p.vault.0, Config::try_from(p.config)?).await?;
            Ok(api::Observed { observation })
        };
        VaultRetire => |p: api::RetireParams| async move {
            Ok(convert::vault(&keeper.retire(p.vault.0, p.force.unwrap_or(false)).await?))
        };
        VaultUpdateSigners => |p: api::UpdateSignersParams| async move {
            Ok(convert::vault(&keeper.update_signers(p.vault.0, p.proposal.0).await?))
        };
        VaultSync => |p: api::VaultParams| async move { keeper.sync(p.vault.0).await };
        VaultObserveTx => |p: api::TxParams| async move {
            Ok(api::TxObserved { known: keeper.observe_tx(p.vault.0, p.tx_hash.0)? })
        };
        PeersStatus => |p: api::VaultParams| async move {
            Ok(keeper.peer_status(p.vault.0).await?.iter().map(convert::reachability).collect())
        };
        MailboxPoll => |_| async {
            let delivered = keeper.poll_mailbox().await?;
            Ok(api::MailboxPolled { delivered: u32::try_from(delivered).unwrap_or(u32::MAX) })
        };
        EcdhBatch => |p: api::EcdhParams| async move {
            let points = p.points.into_iter().map(TryInto::try_into).collect::<Result<Vec<_>, _>>()?;
            let responses = keeper.ecdh_batch(p.vault.0, p.epoch, purpose(p.purpose), points).await?;
            Ok(api::EcdhResult {
                epoch: p.epoch,
                responses: responses.iter().map(convert::member_partials).collect(),
            })
        };
        CeremonyDkg => |p: api::DkgParams| async move {
            keeper
                .start_dkg(DkgParams {
                    session: p.session.0,
                    threshold: p.threshold,
                    members: p.members.into_iter().map(convert::participant).collect(),
                })
                .await
        };
        CeremonyReshare => |p: api::ReshareParams| async move {
            let secrets = p.secrets.map(|s| Zeroizing::new(s.0));
            keeper
                .start_reshare_with_secrets(ReshareRequest {
                    session: p.session.0,
                    vault: p.vault.0,
                    current: p.current.map(TryInto::try_into).transpose()?,
                    dealers: p.dealers,
                    new_threshold: p.new_threshold,
                    new_spending_threshold: p.new_spending_threshold,
                    new_members: p.new_members.into_iter().map(convert::new_member).collect::<Result<_>>()?,
                }, secrets.as_deref().map(Vec::as_slice))
                .await
        };
        CeremonyStatusGet => |p: api::SessionParams| async move {
            let secrets = keeper.joiner_secrets(p.session.0)?;
            convert::ceremony(keeper.ceremony(p.session.0)?, secrets.as_deref().map(Vec::as_slice))
        };
        ProposalsPut => |p: api::BodyParams| async move {
            Ok(api::ProposalStored { id: keeper.put_proposal(p.vault.0, &p.body.0).await?.into() })
        };
        ProposalsList => |p: api::VaultParams| async move { Ok(hex_bodies(keeper.proposals(p.vault.0)?)) };
        ProposalsGet => |p: api::IdParams| async move { Ok(keeper.proposal(p.id.0)?.map(Into::into)) };
        PayloadsPut => |p: api::BodyParams| async move {
            Ok(api::PayloadStored { hash: keeper.put_payload(p.vault.0, &p.body.0).await?.into() })
        };
        PayloadsGet => |p: api::PayloadParams| async move {
            Ok(keeper.payload(p.vault.0, p.hash.0)?.map(Into::into))
        };
        ArtifactsPut => |p: api::ArtifactPutParams| async move {
            convert::artifact_status(keeper.put_artifact(p.class_id.0, p.index, p.count, &p.bytes.0)?)
        };
        ArtifactsGet => |p: api::ArtifactChunkParams| async move {
            Ok(keeper
                .artifact_chunk(p.class_id.0, p.index)?
                .map(|(count, bytes)| api::ArtifactChunk { count, bytes: bytes.into() }))
        };
        ArtifactsStatus => |p: api::ArtifactParams| async move {
            convert::artifact_status(keeper.artifact_progress(p.class_id.0)?)
        };
        ArtifactsFetch => |p: api::ArtifactParams| async move {
            convert::artifact_status(keeper.fetch_artifact(p.vault.0, p.class_id.0).await?)
        };
        ApprovalsPut => |p: api::BodyParams| async move { keeper.put_approval(p.vault.0, &p.body.0).await };
        ApprovalsList => |p: api::ProposalParams| async move { Ok(hex_bodies(keeper.approvals(p.proposal.0)?)) };
        RejectionsPut => |p: api::BodyParams| async move { keeper.put_rejection(p.vault.0, &p.body.0).await };
        RejectionsList => |p: api::ProposalParams| async move {
            Ok(hex_bodies(keeper.rejections(p.proposal.0)?))
        };
        ReceiptsPut => |p: api::ReceiptParams| async move {
            keeper.put_receipt(p.vault.0, p.proposal.0, p.tx_hash.0).await
        };
        ReceiptsList => |p: api::VaultParams| async move {
            Ok(keeper.receipts(p.vault.0)?.iter().map(convert::receipt).collect())
        };
        AuditList => |p: api::AuditParams| async move {
            let limit = p.limit.unwrap_or(100).min(MAX_AUDIT);
            Ok(keeper.audit_log(p.vault.0, limit)?.into_iter().map(convert::audit).collect())
        };
        LobbyOpen => |p| lobby::open(keeper, p);
        LobbyJoin => |p| lobby::join(keeper, p);
        LobbyInspect => |p| async move { lobby::inspect(&p) };
        LobbyStatusGet => |p: api::LobbyParams| async move { lobby::status(keeper, p).await };
        LobbyList => |_| lobby::list(keeper);
        LobbyStart => |p| lobby::start(keeper, p);
        LobbyClose => |p: api::LobbyParams| lobby::close(keeper, p, api::LobbyRole::Host);
        LobbyLeave => |p: api::LobbyParams| lobby::close(keeper, p, api::LobbyRole::Guest);
        DevicesOffer => |p| devices::offer(keeper, p);
        DevicesList => |_| devices::list(keeper);
        DevicesRevoke => |p: api::DeviceParams| async move {
            keeper.revoke_device(ome_wire::IdentityPublic(p.identity.0)).await
        };
        DevicesReview => |p| devices::review(keeper, p);
    }
}
