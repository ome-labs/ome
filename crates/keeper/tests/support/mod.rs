//! Keepers on the in-memory network with a manual clock. Waits follow the keepers' own events, with a time limit.
// Each test binary uses a different subset of these helpers.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::missing_panics_doc
)]

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use ome_api::{
    Hex32,
    keeper::{CeremonyStatus, KeeperEvent},
};
use ome_crypto::{
    Base, Point, SecretScalar,
    aztec::AddressContext,
    schnorr,
    threshold::ecdh::{Partial, combine},
};
use ome_keeper::{
    Clock, DkgResult, Keeper, KeeperConfig, KeeperParts, NewMember, ReshareRequest, ReshareResult,
    ceremony::{Participant, dkg::DkgParams},
    custody::{KeeperKeys, MasterKey, SecretStore},
    store::Store,
};
use ome_net::{Mailbox, MemoryMailbox, MemoryNetwork, Peer};
use ome_protocol::{
    membership::Vault as ProtocolVault,
    payload::{Call, CallKind, ChainInfo, ExecutionRequest, selector},
    proposals::{Draft, approval_message_for},
    vault::{Committee, Config, SignerSet, rejection_message},
};
use ome_wire::{
    Message, encode_body,
    messages::{
        ceremony::{Manifest, Member},
        ecdh::EcdhPurpose,
        proposals::{Approval, Proposal, ProposalPayload, Rejection},
    },
    types::{WirePoint, WireSignature, field},
    version::PROTOCOL_VERSION,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use tempfile::TempDir;
use tokio::sync::broadcast;

/// How long any wait may take before the test fails.
pub const WAIT: Duration = Duration::from_secs(10);

/// A clock tests move by hand.
#[derive(Default)]
pub struct ManualClock(AtomicU64);

impl ManualClock {
    pub fn advance(&self, seconds: u64) {
        self.0.fetch_add(seconds, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> u64 {
        1_700_000_000 + self.0.load(Ordering::SeqCst)
    }
}

pub struct TestKeeper {
    pub keeper: Keeper,
    pub dir: TempDir,
    pub master: [u8; 32],
}

impl TestKeeper {
    pub fn participant(&self) -> Participant {
        Participant {
            identity: self.keeper.identity(),
            transport: self.keeper.transport_key(),
            addresses: vec![],
        }
    }

    pub fn peer(&self) -> Peer {
        Peer {
            identity: self.keeper.identity(),
            transport: self.keeper.transport_key(),
            addresses: vec![],
        }
    }

    /// This keeper's long-lived keys, read from its sealed store (to impersonate it in tests).
    pub fn keys(&self) -> KeeperKeys {
        let secrets = SecretStore::open(
            self.dir.path().join("secrets"),
            &MasterKey::from_bytes(self.master),
        )
        .unwrap();
        KeeperKeys::load_or_create(&secrets).unwrap()
    }

    /// Names of the sealed secrets starting with `prefix`.
    pub fn secrets(&self, prefix: &str) -> Vec<String> {
        let secrets = SecretStore::open(
            self.dir.path().join("secrets"),
            &MasterKey::from_bytes(self.master),
        )
        .unwrap();
        secrets.list(prefix).unwrap()
    }
}

pub struct Harness {
    pub net: Arc<MemoryNetwork>,
    pub clock: Arc<ManualClock>,
    pub config: KeeperConfig,
    pub mailbox: Option<Arc<MemoryMailbox>>,
}

impl Harness {
    pub fn new(config: KeeperConfig) -> Self {
        Self {
            net: MemoryNetwork::new(),
            clock: Arc::new(ManualClock::default()),
            config,
            mailbox: None,
        }
    }

    /// A harness whose keepers share an in-memory relay mailbox.
    pub fn with_mailbox(config: KeeperConfig) -> Self {
        Self {
            mailbox: Some(Arc::new(MemoryMailbox::default())),
            ..Self::new(config)
        }
    }

    pub fn keeper(&self, seed: u8) -> TestKeeper {
        let dir = tempfile::tempdir().unwrap();
        let master = [seed; 32];
        let keeper = self.start(dir.path(), master);
        TestKeeper {
            keeper,
            dir,
            master,
        }
    }

    /// (Re)starts a keeper on `dir`.
    pub fn start(&self, dir: &Path, master: [u8; 32]) -> Keeper {
        let secrets =
            SecretStore::open(dir.join("secrets"), &MasterKey::from_bytes(master)).unwrap();
        let keys = KeeperKeys::load_or_create(&secrets).unwrap();
        let store = Store::open(&dir.join("keeper.db")).unwrap();
        let (transport, inbound) = self.net.endpoint(keys.identity.public());
        Keeper::start(KeeperParts {
            keys,
            secrets,
            store,
            transport: Box::new(transport),
            inbound,
            clock: self.clock.clone(),
            config: self.config.clone(),
            addresses: Vec::new(),
            mailbox: self.mailbox.clone().map(|m| m as Arc<dyn Mailbox>),
        })
        .unwrap()
    }
}

/// The first event matching `wanted`, failing the test after [`WAIT`].
pub async fn wait_for<T>(
    events: &mut broadcast::Receiver<KeeperEvent>,
    mut wanted: impl FnMut(&KeeperEvent) -> Option<T>,
) -> T {
    tokio::time::timeout(WAIT, async {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if let Some(found) = wanted(&event) {
                        return found;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => panic!("keeper stopped"),
            }
        }
    })
    .await
    .expect("event not seen in time")
}

/// Waits until a ceremony `session` reaches `status` on the keeper behind `events`.
pub async fn ceremony_reaches(
    events: &mut broadcast::Receiver<KeeperEvent>,
    session: [u8; 32],
    status: CeremonyStatus,
) {
    let session = Hex32(session);
    wait_for(events, |event| match event {
        KeeperEvent::Ceremony {
            session: s,
            status: got,
            ..
        } if *s == session && *got == status => Some(()),
        _ => None,
    })
    .await;
}

/// Runs a DKG among `keepers` and returns its result.
pub async fn run_dkg(keepers: &[&TestKeeper], threshold: u32, session: [u8; 32]) -> DkgResult {
    let mut events: Vec<_> = keepers.iter().map(|k| k.keeper.subscribe()).collect();
    let params = DkgParams {
        session,
        threshold,
        members: keepers.iter().map(|k| k.participant()).collect(),
    };
    for k in keepers {
        k.keeper.start_dkg(params.clone()).await.unwrap();
    }
    for events in &mut events {
        ceremony_reaches(events, session, CeremonyStatus::Completed).await;
    }
    serde_json::from_value(keepers[0].keeper.ceremony(session).unwrap().result.unwrap()).unwrap()
}

/// The manifest of a vault founded by `keepers` from `result`, the config the contract would store for it, and the
/// members' signing keys in slot order.
pub fn manifest(
    keepers: &[&TestKeeper],
    result: &DkgResult,
    seed: u64,
) -> (Manifest, Config, Vec<SecretScalar>) {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let signers: Vec<SecretScalar> = keepers
        .iter()
        .map(|_| SecretScalar::random(&mut rng))
        .collect();
    let preaddress = Base::from(seed + 1000);
    let group_key = Point::try_from(&result.group_key).unwrap();
    let context = AddressContext::new(preaddress, &group_key).unwrap();
    let members = keepers
        .iter()
        .zip(&result.verification_shares)
        .zip(&signers)
        .zip(1..)
        .map(|(((k, share), signer), index)| Member {
            index,
            signer_key: WirePoint::from(&Point::generator_mul_secret(signer).unwrap()),
            verification_share: *share,
            identity: k.keeper.identity(),
            transport: k.keeper.transport_key(),
            addresses: vec![],
        })
        .collect();
    let manifest = Manifest {
        protocol_version: PROTOCOL_VERSION,
        vault: field(&context.address()),
        epoch: 0,
        threshold: 2,
        viewing_threshold: result.threshold,
        preaddress: field(&preaddress),
        group_key: result.group_key,
        members,
    };
    let config = config_of(&manifest, 0);
    (manifest, config, signers)
}

/// The config note naming `manifest`'s committee at `version`.
pub fn config_of(manifest: &Manifest, version: u32) -> Config {
    let signers = SignerSet::new(manifest.signer_keys().unwrap()).unwrap();
    let committee = Committee::new(
        manifest.viewing_threshold,
        manifest.verification_shares().unwrap(),
    )
    .unwrap();
    Config {
        version,
        ..Config::genesis(&signers, manifest.threshold, &committee).unwrap()
    }
}

pub fn signer_key(rng: &mut ChaCha20Rng) -> WirePoint {
    WirePoint::from(&Point::generator_mul_secret(&SecretScalar::random(rng)).unwrap())
}

/// A new committee member for a reshare.
pub fn new_member(keeper: &TestKeeper, rng: &mut ChaCha20Rng) -> NewMember {
    NewMember {
        identity: keeper.keeper.identity(),
        transport: keeper.keeper.transport_key(),
        addresses: vec![],
        signer_key: signer_key(rng),
    }
}

/// Runs `request` on every participant (`joining` keepers pass the current manifest) and returns the first one's
/// result.
pub async fn run_reshare(
    participants: &[&TestKeeper],
    joining: &[&TestKeeper],
    request: &ReshareRequest,
    current: &Manifest,
) -> ReshareResult {
    let everyone: Vec<&TestKeeper> = participants.iter().chain(joining).copied().collect();
    let mut events: Vec<_> = everyone.iter().map(|k| k.keeper.subscribe()).collect();
    for k in participants {
        k.keeper.start_reshare(request.clone()).await.unwrap();
    }
    for k in joining {
        k.keeper
            .start_reshare(ReshareRequest {
                current: Some(current.clone()),
                ..request.clone()
            })
            .await
            .unwrap();
    }
    for events in &mut events {
        ceremony_reaches(events, request.session, CeremonyStatus::Completed).await;
    }
    serde_json::from_value(
        participants[0]
            .keeper
            .ceremony(request.session)
            .unwrap()
            .result
            .unwrap(),
    )
    .unwrap()
}

/// A random input point.
pub fn point(seed: u64) -> WirePoint {
    WirePoint::from(
        &Point::generator_mul_secret(&SecretScalar::random(&mut ChaCha20Rng::seed_from_u64(seed)))
            .unwrap(),
    )
}

/// `address_secret · P` for each point, combined from a keeper's ECDH batch.
pub async fn ecdh(
    keeper: &Keeper,
    vault: [u8; 32],
    epoch: u32,
    points: &[WirePoint],
) -> Vec<Point> {
    let gathered = keeper
        .ecdh_batch(vault, epoch, EcdhPurpose::NoteDecryption, points.to_vec())
        .await
        .unwrap();
    (0..points.len())
        .map(|i| {
            let partials: Vec<Partial> = gathered.iter().map(|m| m.partials[i]).collect();
            combine(&partials, partials.len()).unwrap()
        })
        .collect()
}

pub fn fast_config() -> KeeperConfig {
    KeeperConfig {
        round_timeout_secs: 30,
        ecdh_timeout: Duration::from_millis(500),
        ..KeeperConfig::default()
    }
}

/// Three keepers, a 2-of-3 committee from a real DKG, registered as a vault.
pub struct Vault {
    pub harness: Harness,
    pub keepers: Vec<TestKeeper>,
    pub address: [u8; 32],
    pub manifest: Manifest,
    pub config: Config,
    pub signers: Vec<SecretScalar>,
}

pub async fn vault(config: KeeperConfig) -> Vault {
    vault_on(Harness::new(config)).await
}

pub async fn vault_on(harness: Harness) -> Vault {
    let keepers: Vec<TestKeeper> = (1..=3).map(|i| harness.keeper(i)).collect();
    let refs: Vec<&TestKeeper> = keepers.iter().collect();
    let result = run_dkg(&refs, 2, [1; 32]).await;
    let (manifest, config, signers) = manifest(&refs, &result, 7);
    for k in &keepers {
        assert!(
            k.keeper
                .register_vault([1; 32], manifest.clone())
                .await
                .unwrap()
                .serving
        );
    }
    Vault {
        harness,
        address: manifest.vault,
        keepers,
        manifest,
        config,
        signers,
    }
}

impl Vault {
    pub fn protocol(&self) -> ProtocolVault {
        ProtocolVault::from_manifest(&self.manifest, &self.config).unwrap()
    }

    /// A one-call transfer proposal of `amount` and its payload, as a member's daemon builds them.
    pub fn proposal(&self, amount: u64) -> (Proposal, ProposalPayload) {
        let signature = "transfer_private_to_private((Field),(Field),u128,Field)";
        let call = Call {
            signature: Some(signature.to_owned()),
            to: field(&Base::from(0x1001u64)),
            selector: selector(signature).unwrap(),
            kind: CallKind::Private,
            is_static: false,
            hide_msg_sender: false,
            args: [0x2001, 0x2002, amount, 7]
                .map(|a| field(&Base::from(a)))
                .to_vec(),
        };
        let draft = Draft {
            request: ExecutionRequest {
                calls: vec![call],
                extras: vec![],
            },
            tx_nonce: Base::from(amount),
            expires_at: 1_800_000_000,
            summary: format!("transfer {amount}"),
            proposer: 1,
        };
        let chain = ChainInfo {
            chain_id: Base::from(31_337u64),
            version: Base::from(1u64),
        };
        draft.build(&self.protocol(), chain).unwrap()
    }

    /// An approval of `proposal` claiming `slot`, signed with member `signer`'s key (a different one forges it).
    pub fn approval(&self, proposal: &Proposal, slot: u32, signer: usize) -> Approval {
        let message = approval_message_for(&self.protocol(), proposal).unwrap();
        let signature = schnorr::sign(
            &self.signers[signer],
            message,
            &mut ChaCha20Rng::seed_from_u64(9),
        )
        .unwrap();
        Approval::new(proposal.id().unwrap(), slot, &signature)
    }

    /// Member `slot` rejects `proposal`.
    pub fn rejection(&self, proposal: &Proposal, slot: u32) -> Rejection {
        let id = proposal.id().unwrap();
        let message = rejection_message(self.protocol().address(), &id);
        let signer = &self.signers[usize::try_from(slot).unwrap()];
        let signature =
            schnorr::sign(signer, message, &mut ChaCha20Rng::seed_from_u64(10)).unwrap();
        Rejection {
            proposal: id,
            slot,
            reason: "no".into(),
            signature: WireSignature::from(&signature),
        }
    }
}

/// A message body as the daemon hands it to its keeper.
pub fn body(message: &Message) -> Vec<u8> {
    encode_body(message).unwrap()
}

/// Whether `frame` is a signed message whose body satisfies `kind`.
pub fn frame_is(frame: &[u8], kind: fn(&Message) -> bool) -> bool {
    ome_wire::decode(frame).is_ok_and(|message| kind(&message.body))
}
