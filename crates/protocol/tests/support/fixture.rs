//! A 2-of-3 vault with a 2-of-3 viewing committee from a real DKG, and helpers to vote on it.

use ome_crypto::{
    Base, Point, SecretScalar,
    aztec::AddressContext,
    schnorr,
    threshold::{
        dkg::{self, KeyShare},
        reshare::{self, SubShare},
    },
};
use ome_protocol::{
    membership::Vault,
    payload::{Call, CallKind, ChainInfo, ExecutionRequest, selector},
    proposals::{Draft, approval_message_for},
    vault::{Committee, Config, SignerSet, rejection_message},
};
use ome_wire::{
    IdentityPublic,
    messages::{
        ceremony::{Manifest, Member},
        proposals::{Approval, Proposal, ProposalPayload, Rejection},
    },
    types::{WirePoint, WireSignature, field},
    version::PROTOCOL_VERSION,
};
use rand_chacha::ChaCha20Rng;

use super::seeded;

pub const NOW: u64 = 1_700_000_000;
pub const PREADDRESS: u64 = 0xabc;

pub fn chain() -> ChainInfo {
    ChainInfo {
        chain_id: Base::from(31_337u64),
        version: Base::from(1u64),
    }
}

/// Everything about a test vault: its members' secrets and what the chain stores.
pub struct Fixture {
    pub signing_keys: Vec<SecretScalar>,
    pub shares: Vec<KeyShare>,
    pub manifest: Manifest,
    pub config: Config,
    pub vault: Vault,
    pub rng: ChaCha20Rng,
}

pub fn fixture() -> Fixture {
    let mut rng = seeded(7);
    let signing_keys: Vec<SecretScalar> = (0..3).map(|_| SecretScalar::random(&mut rng)).collect();
    let shares = run_dkg(2, 3, &mut rng);
    let manifest = manifest(&signing_keys, &shares, 2);
    let config = config_of(&manifest);
    let vault = Vault::from_manifest(&manifest, &config).unwrap();
    Fixture {
        signing_keys,
        shares,
        manifest,
        config,
        vault,
        rng,
    }
}

/// The manifest of a vault whose members sign with `signing_keys` and view with `shares`.
pub fn manifest(signing_keys: &[SecretScalar], shares: &[KeyShare], threshold: u32) -> Manifest {
    let group_key = *shares[0].group_public_key();
    let address = AddressContext::new(Base::from(PREADDRESS), &group_key)
        .unwrap()
        .address();
    Manifest {
        protocol_version: PROTOCOL_VERSION,
        vault: field(&address),
        epoch: 0,
        threshold,
        viewing_threshold: u32::try_from(shares[0].threshold()).unwrap(),
        preaddress: field(&Base::from(PREADDRESS)),
        group_key: WirePoint::from(&group_key),
        members: signing_keys
            .iter()
            .zip(shares)
            .map(|(key, share)| Member {
                index: share.index(),
                signer_key: WirePoint::from(&Point::generator_mul_secret(key).unwrap()),
                verification_share: WirePoint::from(
                    &share.verification_share(share.index()).unwrap(),
                ),
                identity: IdentityPublic([u8::try_from(share.index()).unwrap(); 32]),
                transport: [0; 32],
                addresses: vec![],
            })
            .collect(),
    }
}

/// The config note the contract would store for `manifest`.
pub fn config_of(manifest: &Manifest) -> Config {
    let signers = SignerSet::new(manifest.signer_keys().unwrap()).unwrap();
    let committee = Committee::new(
        manifest.viewing_threshold,
        manifest.verification_shares().unwrap(),
    )
    .unwrap();
    Config::genesis(&signers, manifest.threshold, &committee).unwrap()
}

pub fn run_dkg(t: usize, n: u32, rng: &mut ChaCha20Rng) -> Vec<KeyShare> {
    let parameters = dkg::Parameters {
        session: [7; 32],
        threshold: t,
        participants: (1..=n).collect(),
    };
    let (secrets, round1): (Vec<_>, Vec<_>) = parameters
        .participants
        .iter()
        .map(|&i| dkg::round1(&parameters, i, rng).unwrap())
        .unzip();
    let round2: Vec<_> = secrets
        .iter()
        .flat_map(|s| dkg::round2(&parameters, s, &round1).unwrap())
        .collect();
    secrets
        .into_iter()
        .map(|s| dkg::finalize(&parameters, s, &round1, &round2).unwrap())
        .collect()
}

/// Members `dealers` of `old` reshare to a new committee `1..=new_n` with threshold `new_t`.
pub fn reshare_to(
    old: &[KeyShare],
    dealers: Vec<u32>,
    new_n: u32,
    new_t: usize,
    rng: &mut ChaCha20Rng,
) -> Vec<KeyShare> {
    let parameters = reshare::Parameters {
        session: [9; 32],
        dealers,
        old_threshold: old[0].threshold(),
        new_participants: (1..=new_n).collect(),
        new_threshold: new_t,
    };
    let dealt: Vec<_> = parameters
        .dealers
        .iter()
        .map(|d| {
            reshare::deal(
                &parameters,
                old.iter().find(|s| s.index() == *d).unwrap(),
                rng,
            )
            .unwrap()
        })
        .collect();
    let packages: Vec<_> = dealt.iter().map(|(p, _)| p.clone()).collect();
    (1..=new_n)
        .map(|j| {
            let mine: Vec<SubShare> = dealt
                .iter()
                .flat_map(|(_, subs)| subs)
                .filter(|s| s.recipient == j)
                .map(|s| SubShare {
                    dealer: s.dealer,
                    recipient: j,
                    share: SecretScalar::new(*s.share.expose()),
                })
                .collect();
            reshare::receive(&parameters, old[0].group_commitments(), j, &packages, &mine).unwrap()
        })
        .collect()
}

/// A one-call transfer request.
pub fn request(amount: u64) -> ExecutionRequest {
    let signature = "transfer_private_to_private((Field),(Field),u128,Field)";
    ExecutionRequest {
        calls: vec![Call {
            signature: Some(signature.to_owned()),
            to: field(&Base::from(0x1001u64)),
            selector: selector(signature).unwrap(),
            kind: CallKind::Private,
            is_static: false,
            hide_msg_sender: false,
            args: [0x2001, 0x2002, amount, 7]
                .map(|a| field(&Base::from(a)))
                .to_vec(),
        }],
        extras: vec![],
    }
}

pub fn draft(amount: u64) -> Draft {
    Draft {
        request: request(amount),
        tx_nonce: Base::from(0xabcdu64),
        created_at: NOW,
        expires_at: NOW + 3600,
        summary: format!("transfer {amount}"),
        proposer: 1,
    }
}

pub fn proposed(fixture: &Fixture, amount: u64) -> (Proposal, ProposalPayload) {
    draft(amount).build(&fixture.vault, chain()).unwrap()
}

/// Member in `slot` approves `proposal`.
pub fn approve(fixture: &mut Fixture, proposal: &Proposal, slot: u32) -> Approval {
    let message = approval_message_for(&fixture.vault, proposal).unwrap();
    sign_as(fixture, slot, message, proposal)
}

/// Member in `slot` signs an arbitrary `message` and presents it as an approval of `proposal`.
pub fn sign_as(fixture: &mut Fixture, slot: u32, message: Base, proposal: &Proposal) -> Approval {
    let key = &fixture.signing_keys[usize::try_from(slot).unwrap()];
    let signature = schnorr::sign(key, message, &mut fixture.rng).unwrap();
    Approval::new(proposal.id().unwrap(), slot, &signature)
}

/// Member in `slot` rejects `proposal`.
pub fn reject(fixture: &mut Fixture, proposal: &Proposal, slot: u32) -> Rejection {
    let id = proposal.id().unwrap();
    let key = &fixture.signing_keys[usize::try_from(slot).unwrap()];
    let signature = schnorr::sign(
        key,
        rejection_message(fixture.vault.address(), &id),
        &mut fixture.rng,
    )
    .unwrap();
    Rejection {
        proposal: id,
        slot,
        reason: String::new(),
        signature: WireSignature::from(&signature),
    }
}
