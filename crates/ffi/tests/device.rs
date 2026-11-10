//! The device core against a real relay, with the keeper's side played by the test: an offer pairs the device once and
//! an expired one is refused; reviews arrive through the mailbox; a valid proposal is signable and a tampered one is
//! not; the phone's policy can refuse; and an approval reaches the keeper's mailbox and verifies for the vault.
// One scenario, step by step, against a live relay.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use libp2p::multiaddr::Protocol;
use ome_crypto::{
    Base, Point, SecretScalar,
    aztec::AddressContext,
    sealing::{
        envelope::{TransportPublic, TransportSecret},
        mailbox::{self, mailbox_id},
        pairing::{self, pairing_mailbox},
    },
    threshold::dkg::{self, KeyShare},
};
use ome_ffi::{
    DeviceStore, FfiError, KeyPurpose, OmeDevice, Policy, ReviewKind, StoreError, TokenLimit,
    parse_pair_offer,
};
use ome_net::{Libp2pConfig, Libp2pTransport, Mailbox, RelayMailbox, identity::keypair};
use ome_protocol::{
    membership::Vault,
    payload::{Call, CallKind, ChainInfo, ExecutionRequest, selector},
    proposals::{Draft, verify_approval},
    vault::{Committee, Config, SignerSet},
};
use ome_relay::{Limits, Relay, RelayConfig, store::MailboxStore};
use ome_wire::{
    Header, KeeperIdentity, Message, SignedMessage, decode, encode,
    messages::{
        ceremony::{Manifest, Member},
        device::{
            Ballot, DeviceChain, DeviceConfig, DevicePaired, DeviceReview, PairOffer, PairSecret,
            pairing_scope,
        },
        mailbox::RelayEnvelope,
        proposals::{Proposal, ProposalPayload},
    },
    types::{WirePoint, field},
    version::PROTOCOL_VERSION,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

const DAY: u64 = 86_400;
const TOKEN: u64 = 0x1001;

/// The app's side: files in memory and two fixed sealing keys.
#[derive(Default)]
struct MemoryStore {
    files: Mutex<HashMap<String, Vec<u8>>>,
    deny_signing: Mutex<bool>,
}

impl DeviceStore for MemoryStore {
    fn read(&self, name: String) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.files.lock().unwrap().get(&name).cloned())
    }

    fn write(&self, name: String, bytes: Vec<u8>) -> Result<(), StoreError> {
        self.files.lock().unwrap().insert(name, bytes);
        Ok(())
    }

    fn remove(&self, name: String) -> Result<(), StoreError> {
        self.files.lock().unwrap().remove(&name);
        Ok(())
    }

    fn sealing_key(&self, purpose: KeyPurpose) -> Result<Vec<u8>, StoreError> {
        match purpose {
            KeyPurpose::Transport => Ok(vec![1; 32]),
            KeyPurpose::Signing if *self.deny_signing.lock().unwrap() => Err(StoreError::Denied {
                reason: "cancelled".into(),
            }),
            KeyPurpose::Signing => Ok(vec![2; 32]),
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A message id unique within the test.
fn next_id() -> [u8; 16] {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(
        &NEXT
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .to_be_bytes(),
    );
    id
}

fn hex32(text: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    hex::decode_to_slice(text.trim_start_matches("0x"), &mut out).unwrap();
    out
}

/// The keeper's side: its keys and the relay's mailboxes through a client node.
struct FakeKeeper {
    identity: KeeperIdentity,
    transport: TransportSecret,
    public: TransportPublic,
    mailbox: RelayMailbox,
    node: Arc<Libp2pTransport>,
}

impl FakeKeeper {
    async fn start(relay: ome_net::Multiaddr) -> Self {
        let mut rng = ChaCha20Rng::seed_from_u64(5);
        let identity = KeeperIdentity::generate(&mut rng);
        let (transport, public) = TransportSecret::generate(&mut rng);
        let (node, _) = Libp2pTransport::start(
            &identity,
            Libp2pConfig {
                listen: vec![],
                relays: vec![relay],
                external: vec![],
                request_timeout: Duration::from_secs(5),
                ready_timeout: Duration::from_secs(5),
                reserve: false,
            },
        )
        .await
        .unwrap();
        Self {
            identity,
            transport,
            public,
            mailbox: RelayMailbox::new(Arc::clone(&node)),
            node,
        }
    }

    fn day() -> u32 {
        u32::try_from(now() / DAY).unwrap()
    }

    fn signed(&self, scope: [u8; 32], body: Message) -> Vec<u8> {
        let header = Header {
            scope,
            epoch: 0,
            id: next_id(),
            created_at: now(),
        };
        encode(&SignedMessage::sign(&self.identity, PROTOCOL_VERSION, header, body).unwrap())
            .unwrap()
    }

    async fn send(&self, device: TransportPublic, scope: [u8; 32], body: Message) {
        let frame = self.signed(scope, body);
        let day = Self::day();
        let id = mailbox_id(&self.transport, &device, &self.public, &device, day).unwrap();
        let sealed = mailbox::seal(
            &id,
            day,
            &self.transport,
            &device,
            &frame,
            &mut ChaCha20Rng::seed_from_u64(6),
        )
        .unwrap();
        self.mailbox
            .deposit(RelayEnvelope {
                recipient: id,
                expires_at: now() + DAY,
                sealed,
            })
            .await
            .unwrap();
    }

    async fn receive(&self, device: TransportPublic) -> Vec<SignedMessage> {
        let day = Self::day();
        let id = mailbox_id(&self.transport, &device, &device, &self.public, day).unwrap();
        self.mailbox
            .fetch(vec![id])
            .await
            .unwrap()
            .iter()
            .map(|stored| {
                let frame =
                    mailbox::open(&id, day, &self.transport, &device, &stored.envelope.sealed)
                        .unwrap();
                decode(&frame).unwrap()
            })
            .collect()
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

/// A 2-of-3 vault whose slot 0 is `phone`'s key.
fn vault(phone: WirePoint) -> (Manifest, Config, Vault) {
    let mut rng = ChaCha20Rng::seed_from_u64(9);
    let shares = run_dkg(&mut rng);
    let group_key = *shares[0].group_public_key();
    let preaddress = Base::from(0xabcu64);
    let address = AddressContext::new(preaddress, &group_key)
        .unwrap()
        .address();
    let others: Vec<WirePoint> = (0..2)
        .map(|_| {
            WirePoint::from(&Point::generator_mul_secret(&SecretScalar::random(&mut rng)).unwrap())
        })
        .collect();
    let keys = [phone, others[0], others[1]];
    let manifest = Manifest {
        protocol_version: PROTOCOL_VERSION,
        vault: field(&address),
        epoch: 0,
        threshold: 2,
        viewing_threshold: 2,
        preaddress: field(&preaddress),
        group_key: WirePoint::from(&group_key),
        members: shares
            .iter()
            .zip(keys)
            .map(|(share, key)| Member {
                index: share.index(),
                signer_key: key,
                verification_share: WirePoint::from(
                    &share.verification_share(share.index()).unwrap(),
                ),
                identity: ome_wire::IdentityPublic([u8::try_from(share.index()).unwrap(); 32]),
                transport: [0; 32],
                addresses: vec![],
            })
            .collect(),
    };
    let signers = SignerSet::new(manifest.signer_keys().unwrap()).unwrap();
    let committee = Committee::new(2, manifest.verification_shares().unwrap()).unwrap();
    let config = Config::genesis(&signers, 2, &committee).unwrap();
    let vault = Vault::from_manifest(&manifest, &config).unwrap();
    (manifest, config, vault)
}

fn chain() -> ChainInfo {
    ChainInfo {
        chain_id: Base::from(31_337u64),
        version: Base::from(1u64),
    }
}

fn proposal(vault: &Vault, amount: u64, nonce: u64) -> (Proposal, ProposalPayload) {
    let signature = "transfer_private_to_private((Field),(Field),u128,Field)";
    Draft {
        request: ExecutionRequest {
            calls: vec![Call {
                signature: Some(signature.to_owned()),
                to: field(&Base::from(TOKEN)),
                selector: selector(signature).unwrap(),
                kind: CallKind::Private,
                is_static: false,
                hide_msg_sender: false,
                args: [0x2001, 0x2002, amount, 7]
                    .map(|a| field(&Base::from(a)))
                    .to_vec(),
            }],
            authwits: vec![],
            extras: vec![],
        },
        tx_nonce: Base::from(nonce),
        created_at: now(),
        expires_at: now() + 3600,
        summary: format!("pay {amount}"),
        proposer: 2,
    }
    .build(vault, chain())
    .unwrap()
}

fn review(
    manifest: &Manifest,
    config: &Config,
    proposal: Proposal,
    payload: ProposalPayload,
) -> Message {
    Message::DeviceReview(Box::new(DeviceReview {
        vault: manifest.vault,
        vault_name: "Treasury".into(),
        manifest: manifest.clone(),
        config: DeviceConfig {
            threshold: config.threshold,
            signer_count: config.signer_count,
            version: config.version,
            signer_set_commitment: field(&config.signer_set_commitment),
            viewing_threshold: config.viewing_threshold,
            committee_commitment: field(&config.committee_commitment),
        },
        chain: DeviceChain {
            chain_id: field(&chain().chain_id),
            version: field(&chain().version),
        },
        proposal,
        payload,
        labels: vec![],
        simulation: Some("ok".into()),
        rotation: None,
    }))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_pairs_reviews_and_approves_through_a_relay() {
    let store = MailboxStore::in_memory(Limits::default()).unwrap();
    let mut relay = Relay::start(
        keypair(&KeeperIdentity::from_seed(&[8; 32])).unwrap(),
        store,
        RelayConfig {
            listen: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            ..RelayConfig::default()
        },
    )
    .await
    .unwrap();
    let address = relay.addresses[0]
        .clone()
        .with(Protocol::P2p(relay.peer_id));
    let keeper = FakeKeeper::start(address.clone()).await;
    let app = Arc::new(MemoryStore::default());
    let device = OmeDevice::create(app.clone(), "phone".into()).unwrap();
    let info = device.info();
    let device_transport = TransportPublic(hex32(&info.transport));

    // An expired offer is refused before any network traffic.
    let secret = [3u8; 32];
    let mut offer = PairOffer {
        offer: [4; 16],
        keeper: keeper.identity.public(),
        transport: keeper.public.0,
        relays: vec![address.to_string()],
        secret: PairSecret(secret),
        expires_at: now() - 1,
        member: "Alice".into(),
        network: "aztec:31337:1".into(),
    };
    assert!(matches!(
        device.pair(offer.encode().unwrap(), 5).await,
        Err(FfiError::Refused { .. })
    ));
    offer.expires_at = now() + 600;
    let text = offer.encode().unwrap();
    assert_eq!(parse_pair_offer(&text).unwrap().member, "Alice");

    // The keeper answers the pairing request it finds in the pairing mailbox.
    let pairing = {
        let device = Arc::clone(&device);
        let text = text.clone();
        tokio::spawn(async move { device.pair(text, 30).await })
    };
    let request = loop {
        let found = keeper
            .mailbox
            .fetch(vec![pairing_mailbox(&secret)])
            .await
            .unwrap();
        if let Some(stored) = found.first() {
            break decode(&pairing::open(&secret, &stored.envelope.sealed).unwrap()).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    request.verify().unwrap();
    assert_eq!(request.header.scope, pairing_scope(&offer.offer));
    let Message::DevicePair(pair) = request.body else {
        panic!("not a pairing")
    };
    assert_eq!(pair.transport, device_transport.0);
    keeper
        .send(
            device_transport,
            pairing_scope(&offer.offer),
            Message::DevicePaired(DevicePaired {
                offer: offer.offer,
                member: "Alice".into(),
                network: "aztec:31337:1".into(),
                fee_payers: vec![],
            }),
        )
        .await;
    let paired = pairing.await.unwrap().unwrap();
    assert_eq!(paired.member, "Alice");
    assert!(matches!(
        device.pair(text, 1).await,
        Err(FfiError::Invalid { .. })
    ));

    // Two reviews: a valid payment, and one whose outer hash was tampered with.
    let (manifest, config, vault) = vault(pair.signer_key);
    let (good, good_payload) = proposal(&vault, 50, 1);
    let (mut bad, bad_payload) = proposal(&vault, 60, 2);
    bad.outer_hash = field(&Base::from(1u64));
    for (p, payload) in [(good.clone(), good_payload), (bad.clone(), bad_payload)] {
        keeper
            .send(
                device_transport,
                manifest.vault,
                review(&manifest, &config, p, payload),
            )
            .await;
    }
    let synced = device.sync().await.unwrap();
    assert_eq!(synced.reviews, 2);
    let id = format!("0x{}", hex::encode(good.id().unwrap()));
    let shown = device.review(&id).unwrap();
    assert!(shown.signable, "{:?}", shown.checks);
    assert_eq!(shown.kind, ReviewKind::Payment);
    assert_eq!(
        shown.calls[0].function.as_deref(),
        Some("transfer_private_to_private")
    );
    assert_eq!(shown.calls[0].arguments[2].value, "50");
    let tampered = device
        .review(&format!("0x{}", hex::encode(bad.id().unwrap())))
        .unwrap();
    assert!(!tampered.signable);
    assert!(
        tampered
            .checks
            .iter()
            .any(|c| !c.passed && c.name.contains("hash"))
    );
    assert!(matches!(
        device.approve(tampered.id).await,
        Err(FfiError::Refused { .. })
    ));

    // The phone's own limit refuses the payment; without it the user must still unlock the signing key.
    device
        .set_policy(Policy {
            token_limits: vec![TokenLimit {
                token: format!("{TOKEN:#x}"),
                max_amount: "40".into(),
            }],
            ..Policy::default()
        })
        .unwrap();
    assert!(!device.review(&id).unwrap().signable);
    assert!(matches!(
        device.approve(id.clone()).await,
        Err(FfiError::Refused { .. })
    ));
    device.set_policy(Policy::default()).unwrap();
    *app.deny_signing.lock().unwrap() = true;
    assert!(matches!(
        device.approve(id.clone()).await,
        Err(FfiError::Denied { .. })
    ));
    *app.deny_signing.lock().unwrap() = false;

    // The approval reaches the keeper and verifies for the vault.
    let approved = device.approve(id.clone()).await.unwrap();
    assert!(!approved.signable);
    let votes = keeper.receive(device_transport).await;
    let [vote] = &votes[..] else {
        panic!("expected one vote, got {}", votes.len())
    };
    vote.verify().unwrap();
    let Message::DeviceVote(vote) = &vote.body else {
        panic!("not a vote")
    };
    let Ballot::Approval(approval) = &vote.ballot else {
        panic!("not an approval")
    };
    assert_eq!(approval.slot, 0);
    assert_eq!(verify_approval(&vault, &good, approval), Ok(()));
    assert!(matches!(
        device.approve(id).await,
        Err(FfiError::Refused { .. })
    ));

    // A reopened device keeps its keys, pairing and reviews.
    drop(device);
    let reopened = OmeDevice::open(app).unwrap();
    assert_eq!(reopened.info(), info);
    assert_eq!(reopened.reviews().unwrap().len(), 2);
    keeper.node.stop();
    relay.stop();
}
