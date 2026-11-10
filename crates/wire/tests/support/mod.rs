//! Deterministic sample messages shared by the wire tests: one of every kind, in `Message` declaration order.
// Each test binary uses a different subset of these helpers.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    clippy::too_many_lines
)]

use ome_crypto::{Point, Scalar, SecretScalar};
use ome_wire::{
    Header, KeeperIdentity, Message, SignedMessage,
    messages::{
        artifacts::{ArtifactChunk, ArtifactManifest, ArtifactRequest, chunk_hash},
        ceremony::{
            Abort, AbortReason, Ceremony, Complaint, Confirm, DkgRound1, Echo, Justification,
            Manifest, Member, ReshareDeal, SealedShare,
        },
        device::{
            Ballot, DeviceChain, DeviceConfig, DeviceLabel, DevicePair, DevicePaired, DeviceReview,
            DeviceRotation, DeviceUnpair, DeviceVote, DeviceVoted, PairOffer, PairSecret,
        },
        ecdh::{EcdhPurpose, EcdhRequest, EcdhResponse, WirePartial},
        lobby::{
            Invite, JoinSecret, KeeperContact, LobbyJoin, LobbyLeave, LobbyMember, LobbyPlan,
            LobbyRemoved, LobbyState, SealedSecrets,
        },
        mailbox::RelayEnvelope,
        names::{NameChange, NameSubject},
        proposals::{
            Approval, ClaimPhase, ExecutionClaim, ExecutionReceipt, PayloadRequest, Proposal,
            ProposalPayload, Rejection,
        },
        sync::{SyncRequest, VoteRef},
    },
    types::{Bytes64, WirePoint, WireSignature},
    version::{PROTOCOL_VERSION, SUPPORTED},
};

/// The sample clock.
pub const NOW: u64 = 1_700_000_000;

/// Offset of the body's variant tag in a signed sample: version (1) + scope (32) + epoch (1) + id (16) + timestamp
/// varint (5).
pub const BODY_TAG: usize = 1 + 32 + 1 + 16 + 5;

pub fn point(n: u64) -> Point {
    Point::generator_mul_secret(&SecretScalar::new(Scalar::from(n))).unwrap()
}

pub fn wire_point(n: u64) -> WirePoint {
    WirePoint::from(&point(n))
}

pub fn identity(n: u8) -> KeeperIdentity {
    KeeperIdentity::from_seed(&[n; 32])
}

pub fn header(id: u8) -> Header {
    Header {
        scope: [0x11; 32],
        epoch: 3,
        id: [id; 16],
        created_at: NOW,
    }
}

/// `body` signed by keeper 1 under `header(1)`.
pub fn signed(body: Message) -> SignedMessage {
    SignedMessage::sign(&identity(1), PROTOCOL_VERSION, header(1), body).unwrap()
}

pub fn contact(n: u8) -> KeeperContact {
    KeeperContact {
        identity: identity(n).public(),
        transport: [n; 32],
        versions: SUPPORTED,
        addresses: vec![format!("/ip4/127.0.0.1/udp/410{n}/quic-v1")],
    }
}

pub fn lobby_member(n: u8) -> LobbyMember {
    LobbyMember {
        name: format!("member {n}"),
        account: [n; 32],
        signer_key: wire_point(u64::from(n) + 20),
        keeper: contact(n),
    }
}

pub fn invite() -> Invite {
    Invite {
        lobby: [0x31; 16],
        secret: JoinSecret([0x32; 32]),
        host: contact(1),
        host_name: "Ada".to_owned(),
        name: "Treasury".to_owned(),
        threshold: 2,
        viewing_threshold: 2,
        members: 3,
        network: "aztec-testnet".to_owned(),
    }
}

pub fn manifest() -> Manifest {
    Manifest {
        protocol_version: PROTOCOL_VERSION,
        vault: [0x11; 32],
        epoch: 3,
        threshold: 2,
        viewing_threshold: 2,
        preaddress: [0x03; 32],
        group_key: wire_point(1000),
        members: (1..=3u8)
            .map(|i| Member {
                index: u32::from(i),
                signer_key: wire_point(u64::from(i)),
                verification_share: wire_point(1000 + u64::from(i)),
                identity: identity(i).public(),
                transport: [i; 32],
                addresses: vec![format!("/ip4/127.0.0.1/udp/400{i}/quic-v1")],
            })
            .collect(),
        extensions: ome_wire::Extensions::none(),
    }
}

pub fn payload() -> ProposalPayload {
    ProposalPayload {
        bytes: vec![1, 2, 3, 4],
    }
}

pub fn proposal() -> Proposal {
    Proposal {
        config_version: 3,
        outer_hash: [0x22; 32],
        authwits: vec![[0x23; 32]],
        created_at: NOW,
        expires_at: NOW + 3600,
        tx_nonce: [0x01; 32],
        fee_method: 1,
        payload: payload().reference().unwrap(),
        summary: "transfer 10 to 0x05".to_owned(),
        proposer: 1,
        extensions: ome_wire::Extensions::none(),
    }
}

pub fn lobby_plan() -> LobbyPlan {
    LobbyPlan {
        lobby: [0x31; 16],
        protocol_version: PROTOCOL_VERSION,
        session: [0x41; 32],
        threshold: 2,
        viewing_threshold: 2,
        salt: [0x42; 32],
        members: vec![lobby_member(1), lobby_member(2)],
        secrets: vec![SealedSecrets {
            index: 2,
            encapsulated_key: [0x43; 32],
            ciphertext: vec![0x44; 40],
        }],
    }
}

pub fn relay_envelope() -> RelayEnvelope {
    RelayEnvelope {
        recipient: [0x51; 32],
        expires_at: NOW + 86_400,
        sealed: vec![0x52; 64],
    }
}

/// One sample of every message kind, in `Message` declaration order, named for the vector file.
pub fn samples() -> Vec<(&'static str, Message)> {
    let sealed = SealedShare {
        dealer: 1,
        recipient: 2,
        encapsulated_key: [0x44; 32],
        ciphertext: vec![0x55; 48],
    };
    let confirm = Confirm {
        transcript: Bytes64([0x66; 32], [0x77; 32]),
    };
    let signature = WireSignature([[0x01; 32], [0x02; 32], [0x03; 32], [0x04; 32]]);
    let proposal_id = proposal().id().unwrap();
    vec![
        ("manifest", Message::Manifest(manifest())),
        (
            "dkg_round1",
            Message::DkgRound1(DkgRound1 {
                dealer: 1,
                commitments: vec![wire_point(5), wire_point(6)],
                proof_commitment: wire_point(7),
                proof_response: [0x08; 32],
            }),
        ),
        ("dkg_round2", Message::DkgRound2(sealed.clone())),
        ("dkg_confirm", Message::DkgConfirm(confirm)),
        (
            "reshare_deal",
            Message::ReshareDeal(ReshareDeal {
                dealer: 2,
                commitments: vec![wire_point(9)],
            }),
        ),
        ("reshare_sub_share", Message::ReshareSubShare(sealed)),
        ("reshare_confirm", Message::ReshareConfirm(confirm)),
        (
            "complaint",
            Message::Complaint(Complaint {
                ceremony: Ceremony::Reshare,
                accused: 2,
            }),
        ),
        (
            "justification",
            Message::Justification(Justification {
                ceremony: Ceremony::Dkg,
                complainant: 1,
                share: [0x09; 32],
            }),
        ),
        (
            "abort",
            Message::Abort(Abort {
                ceremony: Ceremony::Dkg,
                reason: AbortReason::Equivocation,
            }),
        ),
        (
            "echo",
            Message::Echo(Echo {
                frame: vec![0x0a; 24],
            }),
        ),
        (
            "ecdh_request",
            Message::EcdhRequest(EcdhRequest {
                purpose: EcdhPurpose::NoteDecryption,
                points: vec![wire_point(11), wire_point(12)],
            }),
        ),
        (
            "ecdh_response",
            Message::EcdhResponse(EcdhResponse {
                request: [0x0b; 16],
                member: 2,
                partials: vec![WirePartial {
                    point: wire_point(13),
                    challenge: [0x0c; 32],
                    response: [0x0d; 32],
                }],
            }),
        ),
        ("proposal", Message::Proposal(proposal())),
        (
            "payload_request",
            Message::PayloadRequest(PayloadRequest {
                hashes: vec![payload().hash()],
            }),
        ),
        ("payload", Message::Payload(payload())),
        (
            "approval",
            Message::Approval(Approval {
                proposal: proposal_id,
                slot: 1,
                signatures: vec![signature, signature],
                extensions: ome_wire::Extensions::none(),
            }),
        ),
        (
            "rejection",
            Message::Rejection(Rejection {
                proposal: proposal_id,
                slot: 2,
                reason: "wrong recipient".to_owned(),
                signature,
                extensions: ome_wire::Extensions::none(),
            }),
        ),
        (
            "execution_receipt",
            Message::ExecutionReceipt(
                ExecutionReceipt::sign(&identity(2), [0x0d; 32], proposal_id, [0x0e; 32], 42)
                    .unwrap(),
            ),
        ),
        (
            "sync_request",
            Message::SyncRequest(SyncRequest {
                proposals: vec![proposal_id],
                approvals: vec![VoteRef {
                    proposal: proposal_id,
                    slot: 0,
                }],
                rejections: vec![],
                receipts: vec![[0x0f; 32]],
            }),
        ),
        (
            "lobby_join",
            Message::LobbyJoin(LobbyJoin {
                lobby: [0x31; 16],
                proof: [0x33; 32],
                member: lobby_member(2),
            }),
        ),
        (
            "lobby_state",
            Message::LobbyState(LobbyState {
                lobby: [0x31; 16],
                revision: 4,
                name: "Treasury".to_owned(),
                threshold: 2,
                viewing_threshold: 2,
                expected: 3,
                members: vec![lobby_member(1), lobby_member(2)],
                started: false,
            }),
        ),
        ("lobby_plan", Message::LobbyPlan(lobby_plan())),
        (
            "lobby_leave",
            Message::LobbyLeave(LobbyLeave { lobby: [0x31; 16] }),
        ),
        (
            "reshare_secrets",
            Message::ReshareSecrets(SealedSecrets {
                index: 3,
                encapsulated_key: [0x41; 32],
                ciphertext: vec![0x42; 48],
            }),
        ),
        (
            "device_pair",
            Message::DevicePair(DevicePair {
                offer: [0x51; 16],
                name: "Alice's phone".to_owned(),
                transport: [0x52; 32],
                signer_key: wire_point(9),
                versions: ome_wire::version::VersionRange { min: 0, max: 0 },
            }),
        ),
        (
            "device_paired",
            Message::DevicePaired(DevicePaired {
                offer: [0x51; 16],
                member: "Alice".to_owned(),
                network: "aztec:31337:0".to_owned(),
                fee_payers: vec![[0x53; 32]],
            }),
        ),
        (
            "device_review",
            Message::DeviceReview(Box::new(device_review())),
        ),
        (
            "device_vote",
            Message::DeviceVote(DeviceVote {
                vault: [0x0a; 32],
                ballot: Ballot::Approval(Approval {
                    proposal: proposal_id,
                    slot: 0,
                    signatures: vec![signature],
                    extensions: ome_wire::Extensions::none(),
                }),
            }),
        ),
        (
            "device_voted",
            Message::DeviceVoted(DeviceVoted {
                proposal: proposal_id,
                refused: Some("vote for an unknown proposal".to_owned()),
            }),
        ),
        (
            "device_unpair",
            Message::DeviceUnpair(DeviceUnpair {
                reason: "revoked".to_owned(),
            }),
        ),
        (
            "artifact_request",
            Message::ArtifactRequest(ArtifactRequest {
                class_id: [0x1a; 32],
                chunks: vec![0, 1],
            }),
        ),
        (
            "artifact_manifest",
            Message::ArtifactManifest(ArtifactManifest {
                class_id: [0x1a; 32],
                chunks: vec![chunk_hash(&[0x1b; 8]), chunk_hash(&[0x1c; 4])],
            }),
        ),
        (
            "artifact_chunk",
            Message::ArtifactChunk(ArtifactChunk {
                class_id: [0x1a; 32],
                index: 1,
                bytes: vec![0x1c; 4],
            }),
        ),
        (
            "execution_claim",
            Message::ExecutionClaim(ExecutionClaim {
                proposal: proposal_id,
                phase: ClaimPhase::Proving,
                started_at: NOW,
            }),
        ),
        (
            "lobby_removed",
            Message::LobbyRemoved(LobbyRemoved { lobby: [0x31; 16] }),
        ),
        (
            "name_change",
            Message::NameChange(
                NameChange::sign(
                    &identity(2),
                    [0x0d; 32],
                    NameSubject::Vault,
                    "Ops".to_owned(),
                    NOW,
                )
                .unwrap(),
            ),
        ),
    ]
}

/// A review of the sample proposal, with a rotation.
pub fn device_review() -> DeviceReview {
    DeviceReview {
        vault: [0x0a; 32],
        vault_name: "Treasury".to_owned(),
        manifest: manifest(),
        config: DeviceConfig {
            threshold: 2,
            signer_count: 3,
            version: 4,
            signer_set_commitment: [0x54; 32],
            viewing_threshold: 2,
            committee_commitment: [0x55; 32],
        },
        chain: DeviceChain {
            chain_id: [0x56; 32],
            version: [0x57; 32],
        },
        proposal: proposal(),
        payload: payload(),
        labels: vec![DeviceLabel {
            address: [0x58; 32],
            label: "Token".to_owned(),
        }],
        simulation: Some("ok".to_owned()),
        rotation: Some(DeviceRotation {
            signers: vec![wire_point(1), wire_point(2)],
            threshold: 2,
            viewing_threshold: 2,
            verification_shares: vec![wire_point(3), wire_point(4)],
        }),
    }
}

/// A sample pairing offer.
pub fn pair_offer() -> PairOffer {
    PairOffer {
        offer: [0x51; 16],
        keeper: identity(1).public(),
        transport: [0x52; 32],
        relays: vec!["/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWExample".to_owned()],
        secret: PairSecret([0x59; 32]),
        expires_at: NOW + 600,
        member: "Alice".to_owned(),
        network: "aztec:31337:0".to_owned(),
    }
}
