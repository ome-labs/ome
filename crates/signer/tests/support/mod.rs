//! A 2-of-3 vault whose members sign with `ome-signer` keys and view with shares from a real DKG.
// Each test binary uses a different subset of these helpers.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::missing_panics_doc
)]

use ome_crypto::{
    Base, Point, SecretScalar,
    aztec::AddressContext,
    threshold::{
        dkg::{self, KeyShare},
        reshare::{self, SubShare},
    },
};
use ome_protocol::{
    membership::{Rotation, Vault},
    payload::{Authwit, Call, CallKind, ChainInfo, ExecutionRequest, selector},
    proposals::Draft,
    vault::{Committee, Config, SignerSet},
};
use ome_signer::{Signer, SigningKey};
use ome_wire::{
    IdentityPublic,
    messages::{
        ceremony::{Manifest, Member},
        proposals::{Proposal, ProposalPayload},
    },
    types::{WirePoint, field},
    version::PROTOCOL_VERSION,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

pub const NOW: u64 = 1_700_000_000;

pub fn seeded(seed: u64) -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(seed)
}

pub fn chain() -> ChainInfo {
    ChainInfo {
        chain_id: Base::from(31_337u64),
        version: Base::from(1u64),
    }
}

pub struct Fixture {
    pub members: Vec<SigningKey>,
    pub shares: Vec<KeyShare>,
    pub vault: Vault,
    pub rng: ChaCha20Rng,
}

pub fn fixture() -> Fixture {
    let mut rng = seeded(11);
    let members: Vec<SigningKey> = (0..3)
        .map(|_| SigningKey::generate(&mut rng).unwrap())
        .collect();
    let shares = run_dkg(&mut rng);
    let group_key = *shares[0].group_public_key();
    let manifest = Manifest {
        protocol_version: PROTOCOL_VERSION,
        vault: field(
            &AddressContext::new(Base::from(0xabcu64), &group_key)
                .unwrap()
                .address(),
        ),
        epoch: 0,
        threshold: 2,
        viewing_threshold: 2,
        preaddress: field(&Base::from(0xabcu64)),
        group_key: WirePoint::from(&group_key),
        members: members
            .iter()
            .zip(&shares)
            .map(|(key, share)| Member {
                index: share.index(),
                signer_key: WirePoint::from(&key.public_key()),
                verification_share: WirePoint::from(
                    &share.verification_share(share.index()).unwrap(),
                ),
                identity: IdentityPublic([u8::try_from(share.index()).unwrap(); 32]),
                transport: [0; 32],
                addresses: vec![],
            })
            .collect(),
    };
    let signers = SignerSet::new(manifest.signer_keys().unwrap()).unwrap();
    let committee = Committee::new(2, manifest.verification_shares().unwrap()).unwrap();
    let config = Config::genesis(&signers, 2, &committee).unwrap();
    let vault = Vault::from_manifest(&manifest, &config).unwrap();
    Fixture {
        members,
        shares,
        vault,
        rng,
    }
}

fn run_dkg(rng: &mut ChaCha20Rng) -> Vec<KeyShare> {
    let parameters = dkg::Parameters {
        session: [7; 32],
        threshold: 2,
        participants: vec![1, 2, 3],
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

/// Member 3 leaves, a newcomer takes slot 3, and members 1 and 2 reshare the viewing key to the new committee.
pub fn rotation(f: &mut Fixture) -> Rotation {
    let parameters = reshare::Parameters {
        session: [9; 32],
        dealers: vec![1, 2],
        old_threshold: 2,
        new_participants: vec![1, 2, 3],
        new_threshold: 2,
    };
    let dealt: Vec<_> = f.shares[..2]
        .iter()
        .map(|s| reshare::deal(&parameters, s, &mut f.rng).unwrap())
        .collect();
    let packages: Vec<_> = dealt.iter().map(|(p, _)| p.clone()).collect();
    let shares: Vec<Point> = (1..=3)
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
            let share = reshare::receive(
                &parameters,
                f.shares[0].group_commitments(),
                j,
                &packages,
                &mine,
            )
            .unwrap();
            share.verification_share(j).unwrap()
        })
        .collect();
    let mut keys = f.vault.signers().keys().to_vec();
    keys[2] = SigningKey::generate(&mut f.rng).unwrap().public_key();
    Rotation {
        signers: SignerSet::new(keys).unwrap(),
        threshold: 2,
        committee: Committee::new(2, shares).unwrap(),
    }
}

pub fn transfer(amount: u64) -> Call {
    let signature = "transfer_private_to_private((Field),(Field),u128,Field)";
    Call {
        signature: Some(signature.to_owned()),
        to: field(&Base::from(0x1001u64)),
        selector: selector(signature).unwrap(),
        kind: CallKind::Private,
        is_static: false,
        hide_msg_sender: false,
        args: [0x2001, 0x2002, amount, 7]
            .map(|a| field(&Base::from(a)))
            .to_vec(),
    }
}

/// A proposal of `calls` on the fixture's vault.
pub fn propose(f: &Fixture, calls: Vec<Call>) -> (Proposal, ProposalPayload) {
    propose_with(f, calls, vec![])
}

/// A proposal of `calls` that also grants `authwits`.
pub fn propose_with(
    f: &Fixture,
    calls: Vec<Call>,
    authwits: Vec<Authwit>,
) -> (Proposal, ProposalPayload) {
    Draft {
        request: ExecutionRequest {
            calls,
            authwits,
            extras: vec![],
        },
        tx_nonce: Base::from(0xabcdu64),
        created_at: NOW,
        expires_at: NOW + 3600,
        summary: String::new(),
        proposer: 1,
    }
    .build(&f.vault, chain())
    .unwrap()
}
