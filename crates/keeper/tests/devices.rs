//! A member's signing device and its keeper, through the relay mailbox: an offer pairs one device once; the keeper
//! sends it proposals to review; its vote is stored and replicated only for the slot whose key it holds; a revoked
//! device is told and ignored; and keepers adopt a signer swap's key only once the chain holds it.
#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use ome_api::keeper::KeeperEvent;
use ome_crypto::{
    Base,
    sealing::{
        envelope::{TransportPublic, TransportSecret},
        mailbox::{self, mailbox_id},
        pairing::{self, pairing_mailbox},
    },
};
use ome_keeper::{Clock, DeviceReviewRequest, Keeper};
use ome_net::{Mailbox, MemoryMailbox};
use ome_protocol::{
    membership::Rotation,
    payload::{Call, CallKind, ChainInfo, ExecutionRequest},
    proposals::Draft,
    vault::constants::{SET_THRESHOLD_SELECTOR, SET_THRESHOLD_SIGNATURE},
};
use ome_wire::{
    Header, KeeperIdentity, Message, SignedMessage, decode, encode,
    messages::{
        device::{Ballot, DeviceChain, DevicePair, DeviceVote, PairOffer, pairing_scope},
        mailbox::RelayEnvelope,
    },
    types::field,
    version::PROTOCOL_VERSION,
};
use rand_chacha::{ChaCha20Rng, rand_core::SeedableRng};
use support::{Harness, Vault, body, config_of, fast_config, vault_on, wait_for};

const DAY: u64 = 86_400;

/// A phone: its identity and transport key, holding member 1's signing key.
struct Phone {
    identity: KeeperIdentity,
    transport: TransportSecret,
    public: TransportPublic,
    keeper: TransportPublic,
    mailbox: Arc<MemoryMailbox>,
    now: u64,
}

impl Phone {
    fn new(seed: u64, keeper: &Keeper, mailbox: Arc<MemoryMailbox>, now: u64) -> Self {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (transport, public) = TransportSecret::generate(&mut rng);
        Self {
            identity: KeeperIdentity::generate(&mut rng),
            transport,
            public,
            keeper: TransportPublic(keeper.transport_key()),
            mailbox,
            now,
        }
    }

    fn signed(&self, scope: [u8; 32], body: Message) -> Vec<u8> {
        let header = Header {
            scope,
            epoch: 0,
            id: rand_id(),
            created_at: self.now,
        };
        encode(&SignedMessage::sign(&self.identity, PROTOCOL_VERSION, header, body).unwrap())
            .unwrap()
    }

    async fn pair(
        &self,
        offer: &PairOffer,
        secret: &[u8; 32],
        signer_key: ome_wire::types::WirePoint,
    ) {
        let frame = self.signed(
            pairing_scope(&offer.offer),
            Message::DevicePair(DevicePair {
                offer: offer.offer,
                name: "phone".into(),
                transport: self.public.0,
                signer_key,
            }),
        );
        let sealed = pairing::seal(secret, &frame, &mut ChaCha20Rng::seed_from_u64(1)).unwrap();
        self.mailbox
            .deposit(RelayEnvelope {
                recipient: pairing_mailbox(secret),
                expires_at: self.now + 600,
                sealed,
            })
            .await
            .unwrap();
    }

    fn day(&self) -> u32 {
        u32::try_from(self.now / DAY).unwrap()
    }

    /// Sends `body` to the keeper through their mailbox.
    async fn send(&self, scope: [u8; 32], body: Message) {
        let frame = self.signed(scope, body);
        let id = mailbox_id(
            &self.transport,
            &self.keeper,
            &self.public,
            &self.keeper,
            self.day(),
        )
        .unwrap();
        let sealed = mailbox::seal(
            &id,
            self.day(),
            &self.transport,
            &self.keeper,
            &frame,
            &mut ChaCha20Rng::seed_from_u64(2),
        )
        .unwrap();
        self.mailbox
            .deposit(RelayEnvelope {
                recipient: id,
                expires_at: self.now + DAY,
                sealed,
            })
            .await
            .unwrap();
    }

    /// Everything the keeper left for this phone, verified, oldest first.
    async fn receive(&self) -> Vec<Message> {
        let id = mailbox_id(
            &self.transport,
            &self.keeper,
            &self.keeper,
            &self.public,
            self.day(),
        )
        .unwrap();
        let found = self.mailbox.fetch(vec![id]).await.unwrap();
        self.mailbox
            .delete(found.iter().map(|e| e.id).collect())
            .await
            .unwrap();
        found
            .iter()
            .map(|stored| {
                let frame = mailbox::open(
                    &id,
                    self.day(),
                    &self.transport,
                    &self.keeper,
                    &stored.envelope.sealed,
                )
                .unwrap();
                let message = decode(&frame).unwrap();
                message.verify().unwrap();
                message.body
            })
            .collect()
    }
}

fn rand_id() -> [u8; 16] {
    static NEXT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
    [NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst); 16]
}

/// A vault whose keepers share a mailbox, and the first keeper's open offer with its secret.
async fn setup() -> (Vault, Arc<MemoryMailbox>, PairOffer) {
    let harness = Harness::with_mailbox(fast_config());
    let mailbox = harness.mailbox.clone().unwrap();
    let v = vault_on(harness).await;
    let (text, _) = v.keepers[0]
        .keeper
        .offer_device("Alice".into(), "aztec:31337:1".into(), vec![[9; 32]])
        .await
        .unwrap();
    (v, mailbox, PairOffer::decode(&text).unwrap())
}

fn review(v: &Vault, proposal: [u8; 32]) -> DeviceReviewRequest {
    DeviceReviewRequest {
        vault: v.address,
        proposal,
        vault_name: "Treasury".into(),
        config: v.config,
        chain: DeviceChain {
            chain_id: field(&Base::from(31_337u64)),
            version: field(&Base::from(1u64)),
        },
        labels: vec![],
        simulation: None,
        rotation: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_offer_pairs_one_device_which_reviews_and_votes() {
    let (v, mailbox, offer) = setup().await;
    let alice = &v.keepers[0].keeper;
    let now = v.harness.clock.now();
    let phone = Phone::new(1, alice, mailbox.clone(), now);
    let mut events = alice.subscribe();

    // A request sealed under another secret is ignored; the real one pairs.
    phone
        .pair(&offer, &[0; 32], v.manifest.members[0].signer_key)
        .await;
    phone
        .pair(&offer, &offer.secret.0, v.manifest.members[0].signer_key)
        .await;
    alice.poll_mailbox().await.unwrap();
    wait_for(&mut events, |e| {
        matches!(e, KeeperEvent::DevicePaired { .. }).then_some(())
    })
    .await;
    assert_eq!(alice.devices().await.len(), 1);
    let paired = phone.receive().await;
    assert!(
        matches!(&paired[..], [Message::DevicePaired(p)] if p.member == "Alice" && p.fee_payers == [[9; 32]])
    );
    // The offer is spent: a second device cannot use it.
    let other = Phone::new(2, alice, mailbox.clone(), now);
    other
        .pair(&offer, &offer.secret.0, v.manifest.members[1].signer_key)
        .await;
    alice.poll_mailbox().await.unwrap();
    assert_eq!(alice.devices().await.len(), 1);

    let (proposal, payload) = v.proposal(20);
    let id = proposal.id().unwrap();
    alice
        .put_payload(v.address, &body(&Message::Payload(payload)))
        .await
        .unwrap();
    alice
        .put_proposal(v.address, &body(&Message::Proposal(proposal.clone())))
        .await
        .unwrap();
    assert_eq!(alice.review_on_devices(review(&v, id)).await.unwrap(), 1);
    let received = phone.receive().await;
    let [Message::DeviceReview(sent)] = &received[..] else {
        panic!("no review: {received:?}")
    };
    assert_eq!(sent.proposal, proposal);
    assert_eq!(sent.manifest, v.manifest);

    // A vote for a slot whose key the phone does not hold is refused.
    phone
        .send(
            v.address,
            Message::DeviceVote(DeviceVote {
                vault: v.address,
                ballot: Ballot::Approval(v.approval(&proposal, 1, 1)),
            }),
        )
        .await;
    alice.poll_mailbox().await.unwrap();
    let refused = wait_for(&mut events, |e| match e {
        KeeperEvent::DeviceVote { refused, .. } => Some(refused.clone()),
        _ => None,
    })
    .await;
    assert!(refused.is_some());
    assert_eq!(alice.approvals(id).unwrap().len(), 0);

    // Its own slot's vote is stored and replicated.
    let mut bob = v.keepers[1].keeper.subscribe();
    phone
        .send(
            v.address,
            Message::DeviceVote(DeviceVote {
                vault: v.address,
                ballot: Ballot::Approval(v.approval(&proposal, 0, 0)),
            }),
        )
        .await;
    alice.poll_mailbox().await.unwrap();
    wait_for(&mut bob, |e| {
        matches!(e, KeeperEvent::Approval { slot: 0, .. }).then_some(())
    })
    .await;
    let answers = phone.receive().await;
    assert!(
        answers
            .iter()
            .any(|m| matches!(m, Message::DeviceVoted(a) if a.refused.is_none()))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_device_is_told_and_ignored() {
    let (v, mailbox, offer) = setup().await;
    let alice = &v.keepers[0].keeper;
    let phone = Phone::new(3, alice, mailbox, v.harness.clock.now());
    phone
        .pair(&offer, &offer.secret.0, v.manifest.members[0].signer_key)
        .await;
    alice.poll_mailbox().await.unwrap();
    phone.receive().await;
    let device = alice.devices().await[0].identity;

    alice.revoke_device(device).await.unwrap();
    assert!(matches!(
        &phone.receive().await[..],
        [Message::DeviceUnpair(_)]
    ));

    let (proposal, payload) = v.proposal(21);
    alice
        .put_payload(v.address, &body(&Message::Payload(payload)))
        .await
        .unwrap();
    alice
        .put_proposal(v.address, &body(&Message::Proposal(proposal.clone())))
        .await
        .unwrap();
    assert_eq!(
        alice
            .review_on_devices(review(&v, proposal.id().unwrap()))
            .await
            .unwrap(),
        0
    );
    phone
        .send(
            v.address,
            Message::DeviceVote(DeviceVote {
                vault: v.address,
                ballot: Ballot::Approval(v.approval(&proposal, 0, 0)),
            }),
        )
        .await;
    alice.poll_mailbox().await.unwrap();
    assert_eq!(alice.approvals(proposal.id().unwrap()).unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn keepers_adopt_a_swapped_signer_once_the_chain_holds_it() {
    let (v, _, _) = setup().await;
    let phone_key = support::signer_key(&mut ChaCha20Rng::seed_from_u64(77));
    let rotation = Rotation::signer_swap(
        &v.protocol(),
        0,
        ome_crypto::Point::try_from(&phone_key).unwrap(),
    )
    .unwrap();
    let (proposal, payload) = Draft {
        request: ExecutionRequest {
            calls: vec![rotation.to_call(&v.protocol())],
            extras: vec![],
        },
        tx_nonce: Base::from(99u64),
        created_at: 1_700_000_000,
        expires_at: 1_800_000_000,
        summary: "swap".into(),
        proposer: 1,
    }
    .build(
        &v.protocol(),
        ChainInfo {
            chain_id: Base::from(31_337u64),
            version: Base::from(1u64),
        },
    )
    .unwrap();
    let bob = &v.keepers[1].keeper;
    let mut events = bob.subscribe();
    v.keepers[0]
        .keeper
        .put_payload(v.address, &body(&Message::Payload(payload)))
        .await
        .unwrap();
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal.clone())))
        .await
        .unwrap();
    wait_for(&mut events, |e| {
        matches!(e, KeeperEvent::Payload { .. }).then_some(())
    })
    .await;
    let id = proposal.id().unwrap();

    // The chain still holds the old signers.
    bob.observe_config(v.address, v.config).await.unwrap();
    assert!(bob.update_signers(v.address, id).await.is_err());

    let mut swapped = v.manifest.clone();
    swapped.members[0].signer_key = phone_key;
    let landed = config_of(&swapped, v.config.version + 1);
    bob.observe_config(v.address, landed).await.unwrap();
    let info = bob.update_signers(v.address, id).await.unwrap();
    assert_eq!(info.manifest.members[0].signer_key, phone_key);
    assert_eq!(info.manifest.epoch, v.manifest.epoch);
    assert!(info.serving);
}

#[tokio::test(flavor = "multi_thread")]
async fn keepers_adopt_a_new_threshold_once_the_chain_holds_it() {
    let (v, _, _) = setup().await;
    let (proposal, payload) = Draft {
        request: ExecutionRequest {
            calls: vec![Call {
                signature: Some(SET_THRESHOLD_SIGNATURE.to_owned()),
                to: field(&v.protocol().address()),
                selector: SET_THRESHOLD_SELECTOR,
                kind: CallKind::Private,
                is_static: false,
                hide_msg_sender: false,
                args: vec![field(&Base::from(3u64))],
            }],
            extras: vec![],
        },
        tx_nonce: Base::from(98u64),
        created_at: 1_700_000_000,
        expires_at: 1_800_000_000,
        summary: "threshold".into(),
        proposer: 1,
    }
    .build(
        &v.protocol(),
        ChainInfo {
            chain_id: Base::from(31_337u64),
            version: Base::from(1u64),
        },
    )
    .unwrap();
    let bob = &v.keepers[1].keeper;
    let mut events = bob.subscribe();
    v.keepers[0]
        .keeper
        .put_payload(v.address, &body(&Message::Payload(payload)))
        .await
        .unwrap();
    v.keepers[0]
        .keeper
        .put_proposal(v.address, &body(&Message::Proposal(proposal.clone())))
        .await
        .unwrap();
    wait_for(&mut events, |e| {
        matches!(e, KeeperEvent::Payload { .. }).then_some(())
    })
    .await;
    let id = proposal.id().unwrap();

    // The chain still holds the old threshold.
    bob.observe_config(v.address, v.config).await.unwrap();
    assert!(bob.update_signers(v.address, id).await.is_err());

    let mut raised = v.manifest.clone();
    raised.threshold = 3;
    bob.observe_config(v.address, config_of(&raised, v.config.version + 1))
        .await
        .unwrap();
    let info = bob.update_signers(v.address, id).await.unwrap();
    assert_eq!(info.manifest.threshold, 3);
    assert_eq!(info.manifest.members, v.manifest.members);
    assert!(info.serving);
}
