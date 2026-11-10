//! `OmeDevice`: a member's signing device. It pairs with the member's keeper, receives proposals to review, and signs
//! approvals and rejections only after every check passed. Network calls run on the device's own runtime, so the app
//! may await them from any executor.

use std::{
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use ome_crypto::sealing::envelope::TransportPublic;
use ome_net::identity::peer_id;
use ome_wire::{
    IdentityPublic, Message, SignedMessage, decode_body, encode_body,
    messages::device::{
        Ballot, DevicePair, DeviceReview, DeviceUnpair, DeviceVote, PairOffer, pairing_scope,
    },
    types::WireField,
};
use serde::{Deserialize, Serialize};

use crate::{
    FfiError, Policy,
    error::Result,
    keys::{DeviceKeys, rng},
    link::{Link, Peer, sign, unix_now},
    review::{Analysis, Review, ReviewKind, ReviewStatus, analyze},
    store::{DeviceStore, Store, read_json, write_json},
};

const PAIRING_FILE: &str = "pairing.json";
const REVIEWS_FILE: &str = "reviews.json";
const POLICY_FILE: &str = "policy.json";
/// Most reviews kept; the oldest are dropped first.
const MAX_REVIEWS: usize = 64;
/// How often `pair` checks for the keeper's answer.
const PAIR_POLL: Duration = Duration::from_secs(2);

/// A point as hex coordinates.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PublicKey {
    /// `x` (hex).
    pub x: String,
    /// `y` (hex).
    pub y: String,
}

/// Who this device is.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DeviceInfo {
    /// Display name.
    pub name: String,
    /// Ed25519 identity (hex).
    pub identity: String,
    /// X25519 transport key (hex).
    pub transport: String,
    /// The Grumpkin key it signs approvals with: what a signer swap puts in the member's slot.
    pub signer_key: PublicKey,
    /// Its libp2p peer id, for a private relay's allow list.
    pub peer_id: String,
}

/// A pairing offer, read before pairing.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct OfferInfo {
    /// The member's display name.
    pub member: String,
    /// Network label.
    pub network: String,
    /// The keeper's identity (hex).
    pub keeper: String,
    /// The relays both sides use.
    pub relays: Vec<String>,
    /// Unix time after which the keeper refuses it.
    pub expires_at: u64,
}

/// The keeper this device is paired with.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PairingInfo {
    /// The keeper's identity (hex).
    pub keeper: String,
    /// The member's display name.
    pub member: String,
    /// Network label.
    pub network: String,
    /// The relays both sides use.
    pub relays: Vec<String>,
    /// When it paired (unix seconds).
    pub paired_at: u64,
}

/// What a sync brought.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SyncResult {
    /// New or updated reviews.
    pub reviews: u32,
    /// Answers to this device's votes.
    pub outcomes: u32,
    /// Whether the keeper ended the pairing.
    pub unpaired: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairingFile {
    keeper: [u8; 32],
    transport: [u8; 32],
    relays: Vec<String>,
    member: String,
    network: String,
    fee_payers: Vec<WireField>,
    paired_at: u64,
}

impl PairingFile {
    fn peer(&self) -> Peer {
        Peer {
            identity: IdentityPublic(self.keeper),
            transport: TransportPublic(self.transport),
            relays: self.relays.clone(),
        }
    }

    fn info(&self) -> PairingInfo {
        PairingInfo {
            keeper: hex32(&self.keeper),
            member: self.member.clone(),
            network: self.network.clone(),
            relays: self.relays.clone(),
            paired_at: self.paired_at,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Vote {
    Approved,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewEntry {
    /// Hex of the review's canonical body.
    body: String,
    received_at: u64,
    vote: Option<Vote>,
    outcome: Option<String>,
}

impl ReviewEntry {
    fn review(&self) -> Result<DeviceReview> {
        let bytes =
            hex::decode(&self.body).map_err(|_| FfiError::storage("a stored review is damaged"))?;
        match decode_body(&bytes)? {
            Message::DeviceReview(review) => Ok(*review),
            _ => Err(FfiError::storage("a stored review is damaged")),
        }
    }
}

/// A member's signing device.
#[derive(uniffi::Object)]
pub struct OmeDevice {
    store: Store,
    keys: Arc<DeviceKeys>,
    runtime: Option<tokio::runtime::Runtime>,
    /// Serializes read-modify-write of the device's files.
    files: Mutex<()>,
}

impl Drop for OmeDevice {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

#[uniffi::export]
impl OmeDevice {
    /// Creates a device in an empty store: its identity, transport and signing keys, sealed under the app's sealing
    /// keys. Asks for both, so the user is asked once.
    ///
    /// # Errors
    /// [`FfiError::Invalid`] if the store already holds a device, [`FfiError::Denied`], [`FfiError::Storage`].
    #[uniffi::constructor]
    pub fn create(store: Arc<dyn DeviceStore>, name: String) -> Result<Arc<Self>> {
        let keys = DeviceKeys::create(&store, name)?;
        Self::with(store, keys)
    }

    /// Opens the device in `store` with the transport sealing key; signing asks for the signing key later.
    ///
    /// # Errors
    /// [`FfiError::NotPaired`] for an empty store, [`FfiError::Denied`], [`FfiError::Storage`].
    #[uniffi::constructor]
    pub fn open(store: Arc<dyn DeviceStore>) -> Result<Arc<Self>> {
        let keys = DeviceKeys::open(&store)?;
        Self::with(store, keys)
    }

    /// Who this device is.
    #[must_use]
    pub fn info(&self) -> DeviceInfo {
        let public = &self.keys.public;
        DeviceInfo {
            name: public.name.clone(),
            identity: hex32(&public.identity),
            transport: hex32(&public.transport),
            signer_key: PublicKey {
                x: hex32(&public.signer_key.x),
                y: hex32(&public.signer_key.y),
            },
            peer_id: peer_id(&self.keys.identity_public())
                .map(|id| id.to_string())
                .unwrap_or_default(),
        }
    }

    /// The keeper this device is paired with.
    ///
    /// # Errors
    /// [`FfiError::Storage`].
    pub fn pairing(&self) -> Result<Option<PairingInfo>> {
        Ok(self.pairing_file()?.map(|p| p.info()))
    }

    /// Every received proposal, newest first, with this phone's checks run now.
    ///
    /// # Errors
    /// [`FfiError::Storage`].
    pub fn reviews(&self) -> Result<Vec<Review>> {
        let pairing = self.pairing_file()?;
        let policy = self.policy()?;
        let fee_payers = pairing.map(|p| p.fee_payers).unwrap_or_default();
        let now = unix_now();
        let mut reviews = self
            .entries()?
            .iter()
            .map(|entry| self.view(entry, &policy, &fee_payers, now))
            .collect::<Result<Vec<_>>>()?;
        reviews.sort_by_key(|r| std::cmp::Reverse(r.expires_at));
        Ok(reviews)
    }

    /// One received proposal.
    ///
    /// # Errors
    /// [`FfiError::Invalid`] for an unknown id, [`FfiError::Storage`].
    pub fn review(&self, id: &str) -> Result<Review> {
        self.reviews()?
            .into_iter()
            .find(|r| r.id == normalized(id))
            .ok_or_else(|| FfiError::invalid(format!("no proposal {id} on this device")))
    }

    /// This phone's policy.
    ///
    /// # Errors
    /// [`FfiError::Storage`].
    pub fn policy(&self) -> Result<Policy> {
        Ok(read_json(&self.store, POLICY_FILE)?.unwrap_or_default())
    }

    /// Replaces this phone's policy.
    ///
    /// # Errors
    /// [`FfiError::Invalid`] for an address or amount that does not parse, [`FfiError::Storage`].
    #[allow(
        clippy::needless_pass_by_value,
        reason = "UniFFI passes records by value"
    )]
    pub fn set_policy(&self, policy: Policy) -> Result<()> {
        policy.validate()?;
        write_json(&self.store, POLICY_FILE, &policy)
    }
}

#[uniffi::export]
impl OmeDevice {
    /// Pairs with the keeper that made `offer`, waiting up to `wait_secs` for its answer (keepers check their
    /// mailboxes about every 30 seconds).
    ///
    /// # Errors
    /// [`FfiError::Invalid`] for a malformed offer or an existing pairing, [`FfiError::Refused`] for an expired offer,
    /// [`FfiError::Network`] if the keeper does not answer in time.
    pub async fn pair(&self, offer: String, wait_secs: u32) -> Result<PairingInfo> {
        let offer = PairOffer::decode(&offer)?;
        if offer.expires_at < unix_now() {
            return Err(FfiError::refused("this pairing offer has expired"));
        }
        if self.pairing_file()?.is_some() {
            return Err(FfiError::invalid(
                "this device is already paired; unpair it first",
            ));
        }
        let frame = sign(
            &self.keys,
            pairing_scope(&offer.offer),
            0,
            Message::DevicePair(DevicePair {
                offer: offer.offer,
                name: self.keys.public.name.clone(),
                transport: self.keys.public.transport,
                signer_key: self.keys.public.signer_key,
                versions: ome_wire::version::SUPPORTED,
            }),
        )?;
        let peer = Peer {
            identity: offer.keeper,
            transport: TransportPublic(offer.transport),
            relays: offer.relays.clone(),
        };
        let keys = Arc::clone(&self.keys);
        let wait = Duration::from_secs(u64::from(wait_secs));
        let id = offer.offer;
        let (paired, rest) = self
            .run({
                let peer = peer.clone();
                async move {
                    let link = Link::open(&keys, &peer.relays).await?;
                    link.deposit_pairing(&offer, &frame).await?;
                    let deadline = tokio::time::Instant::now() + wait;
                    loop {
                        let mut messages = link.receive(&keys, &peer).await?;
                        if let Some(at) = messages.iter().position(
                            |m| matches!(&m.body, Message::DevicePaired(p) if p.offer == id),
                        ) {
                            let paired = messages.remove(at);
                            return Ok((paired, messages));
                        }
                        if tokio::time::Instant::now() >= deadline {
                            return Err(FfiError::network(
                                "the keeper did not answer the pairing in time; keep the app open and try again",
                            ));
                        }
                        tokio::time::sleep(PAIR_POLL).await;
                    }
                }
            })
            .await?;
        let Message::DevicePaired(paired) = paired.body else {
            return Err(FfiError::network("the keeper answered something else"));
        };
        let pairing = PairingFile {
            keeper: peer.identity.0,
            transport: peer.transport.0,
            relays: peer.relays,
            member: paired.member,
            network: paired.network,
            fee_payers: paired.fee_payers,
            paired_at: unix_now(),
        };
        write_json(&self.store, PAIRING_FILE, &pairing)?;
        self.absorb(rest)?;
        Ok(pairing.info())
    }

    /// Fetches what the keeper sent: new proposals to review, answers to votes, or the end of the pairing.
    ///
    /// # Errors
    /// [`FfiError::NotPaired`], [`FfiError::Network`], [`FfiError::Storage`].
    pub async fn sync(&self) -> Result<SyncResult> {
        let pairing = self.paired()?;
        let keys = Arc::clone(&self.keys);
        let peer = pairing.peer();
        let messages = self
            .run(async move {
                let link = Link::open(&keys, &peer.relays).await?;
                link.receive(&keys, &peer).await
            })
            .await?;
        self.absorb(messages)
    }

    /// Approves a proposal after running every check again, opening the signing key (the app asks the user) for this
    /// one signature, and sends the approval to the keeper.
    ///
    /// # Errors
    /// [`FfiError::Refused`] if a check fails or the phone already voted, [`FfiError::Denied`],
    /// [`FfiError::Network`].
    pub async fn approve(&self, id: String) -> Result<Review> {
        self.vote(&id, None).await
    }

    /// Rejects a proposal with a reason shown to the other members, and sends the rejection to the keeper.
    ///
    /// # Errors
    /// [`FfiError::Refused`] if the phone already voted or the vault is not readable, [`FfiError::Denied`],
    /// [`FfiError::Network`].
    pub async fn reject(&self, id: String, reason: String) -> Result<Review> {
        self.vote(&id, Some(reason)).await
    }

    /// Ends the pairing: tells the keeper, then forgets it and every review.
    ///
    /// # Errors
    /// [`FfiError::NotPaired`], [`FfiError::Network`], [`FfiError::Storage`].
    pub async fn unpair(&self) -> Result<()> {
        let pairing = self.paired()?;
        let frame = sign(
            &self.keys,
            [0; 32],
            0,
            Message::DeviceUnpair(DeviceUnpair {
                reason: "the device ended the pairing".into(),
            }),
        )?;
        let keys = Arc::clone(&self.keys);
        let peer = pairing.peer();
        self.run(async move {
            let link = Link::open(&keys, &peer.relays).await?;
            link.send(&keys, &peer, &frame).await
        })
        .await?;
        self.forget()
    }
}

impl OmeDevice {
    fn with(store: Store, keys: DeviceKeys) -> Result<Arc<Self>> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ome-device")
            .enable_all()
            .build()
            .map_err(|e| FfiError::storage(format!("cannot start the device runtime: {e}")))?;
        Ok(Arc::new(Self {
            store,
            keys: Arc::new(keys),
            runtime: Some(runtime),
            files: Mutex::new(()),
        }))
    }

    /// Runs `future` on the device's runtime; the returned future may be awaited from any executor.
    async fn run<T: Send + 'static>(
        &self,
        future: impl Future<Output = Result<T>> + Send + 'static,
    ) -> Result<T> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| FfiError::network("the device is shutting down"))?;
        runtime
            .spawn(future)
            .await
            .map_err(|e| FfiError::network(format!("the network task failed: {e}")))?
    }

    fn pairing_file(&self) -> Result<Option<PairingFile>> {
        read_json(&self.store, PAIRING_FILE)
    }

    fn paired(&self) -> Result<PairingFile> {
        self.pairing_file()?
            .ok_or_else(|| FfiError::not_paired("this device is not paired with a keeper"))
    }

    fn entries(&self) -> Result<Vec<ReviewEntry>> {
        Ok(read_json(&self.store, REVIEWS_FILE)?.unwrap_or_default())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.files.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn forget(&self) -> Result<()> {
        let _guard = self.lock();
        self.store.remove(PAIRING_FILE.into())?;
        self.store.remove(REVIEWS_FILE.into())?;
        Ok(())
    }

    /// Takes in what the keeper sent.
    fn absorb(&self, messages: Vec<SignedMessage>) -> Result<SyncResult> {
        let mut result = SyncResult {
            reviews: 0,
            outcomes: 0,
            unpaired: false,
        };
        let guard = self.lock();
        let mut entries = self.entries()?;
        for message in messages {
            match message.body {
                Message::DeviceReview(review) if review.vault == message.header.scope => {
                    let id = review.proposal.id()?;
                    let body = hex::encode(encode_body(&Message::DeviceReview(review))?);
                    match entries.iter_mut().find(|e| entry_id(e) == Some(id)) {
                        Some(entry) => entry.body = body,
                        None => entries.push(ReviewEntry {
                            body,
                            received_at: unix_now(),
                            vote: None,
                            outcome: None,
                        }),
                    }
                    result.reviews += 1;
                }
                Message::DeviceVoted(voted) => {
                    if let Some(entry) = entries
                        .iter_mut()
                        .find(|e| entry_id(e) == Some(voted.proposal))
                    {
                        entry.outcome = Some(
                            voted
                                .refused
                                .map_or_else(|| "accepted".to_owned(), |r| format!("refused: {r}")),
                        );
                        result.outcomes += 1;
                    }
                }
                Message::DeviceUnpair(_) => result.unpaired = true,
                _ => {}
            }
        }
        if entries.len() > MAX_REVIEWS {
            entries.sort_by_key(|e| e.received_at);
            let excess = entries.len() - MAX_REVIEWS;
            entries.drain(..excess);
        }
        write_json(&self.store, REVIEWS_FILE, &entries)?;
        drop(guard);
        if result.unpaired {
            self.forget()?;
        }
        Ok(result)
    }

    fn view(
        &self,
        entry: &ReviewEntry,
        policy: &Policy,
        fee_payers: &[WireField],
        now: u64,
    ) -> Result<Review> {
        let review = entry.review()?;
        let analysis = analyze(&review, &self.keys.signer_key()?, policy, fee_payers, now);
        Ok(render(&review, entry, &analysis, now))
    }

    /// Signs the vote, opening the signing key for this one signature.
    fn ballot(
        &self,
        review: &DeviceReview,
        analysis: &Analysis,
        pairing: &PairingFile,
        rejection: Option<&str>,
        now: u64,
    ) -> Result<Ballot> {
        let (Some(vault), Some(chain)) = (&analysis.vault, analysis.chain) else {
            return Err(FfiError::refused(
                "the vault in this review is not readable",
            ));
        };
        let signer = DeviceKeys::signing_key(&self.store)?;
        let mut rng = rng();
        let ballot = match (rejection, analysis.kind) {
            (Some(reason), _) => Ballot::Rejection(ome_signer::reject(
                &signer,
                vault,
                &review.proposal,
                reason,
                &mut rng,
            )?),
            (None, ReviewKind::Payment | ReviewKind::Cancellation) => {
                Ballot::Approval(ome_signer::approve(
                    &signer,
                    vault,
                    chain,
                    &review.proposal,
                    &review.payload,
                    now,
                    &mut rng,
                )?)
            }
            (None, ReviewKind::Rotation) => {
                let rotation = analysis
                    .rotation
                    .as_ref()
                    .ok_or_else(|| FfiError::refused("the rotation does not decode"))?;
                Ballot::Approval(ome_signer::approve_rotation(
                    &signer,
                    vault,
                    chain,
                    &review.proposal,
                    &review.payload,
                    rotation,
                    &pairing.fee_payers,
                    now,
                    &mut rng,
                )?)
            }
            (None, ReviewKind::SignerSwap) => Ballot::Approval(
                ome_signer::approve_signer_swap(
                    &signer,
                    vault,
                    chain,
                    &review.proposal,
                    &review.payload,
                    &pairing.fee_payers,
                    now,
                    &mut rng,
                )?
                .0,
            ),
        };
        Ok(ballot)
    }

    async fn vote(&self, id: &str, rejection: Option<String>) -> Result<Review> {
        let pairing = self.paired()?;
        let entry = self
            .entries()?
            .into_iter()
            .find(|e| entry_id(e).map(|i| hex32(&i)) == Some(normalized(id)))
            .ok_or_else(|| FfiError::invalid(format!("no proposal {id} on this device")))?;
        if entry.vote.is_some() {
            return Err(FfiError::refused(
                "this phone already voted on this proposal",
            ));
        }
        let review = entry.review()?;
        let now = unix_now();
        let analysis = analyze(
            &review,
            &self.keys.signer_key()?,
            &self.policy()?,
            &pairing.fee_payers,
            now,
        );
        if analysis.vault.is_none() {
            return Err(FfiError::refused(
                "the vault in this review is not readable",
            ));
        }
        if rejection.is_none()
            && let Some(failed) = analysis.checks.iter().find(|c| !c.passed)
        {
            return Err(FfiError::refused(format!(
                "{}: {}",
                failed.name,
                failed.detail.clone().unwrap_or_default()
            )));
        }
        let ballot = self.ballot(&review, &analysis, &pairing, rejection.as_deref(), now)?;
        let frame = sign(
            &self.keys,
            review.vault,
            review.manifest.epoch,
            Message::DeviceVote(DeviceVote {
                vault: review.vault,
                ballot,
            }),
        )?;
        let keys = Arc::clone(&self.keys);
        let peer = pairing.peer();
        self.run(async move {
            let link = Link::open(&keys, &peer.relays).await?;
            link.send(&keys, &peer, &frame).await
        })
        .await?;
        let vote = if rejection.is_some() {
            Vote::Rejected
        } else {
            Vote::Approved
        };
        let entry = {
            let _guard = self.lock();
            let mut entries = self.entries()?;
            let stored = entries
                .iter_mut()
                .find(|e| e.body == entry.body)
                .ok_or_else(|| FfiError::storage("the review disappeared while voting"))?;
            stored.vote = Some(vote);
            stored.outcome = None;
            let updated = stored.clone();
            write_json(&self.store, REVIEWS_FILE, &entries)?;
            updated
        };
        let policy = self.policy()?;
        self.view(&entry, &policy, &pairing.fee_payers, now)
    }
}

/// Reads an offer without pairing, to show whose keeper it is.
///
/// # Errors
/// [`FfiError::Invalid`] for a malformed offer.
#[uniffi::export]
pub fn parse_pair_offer(offer: &str) -> Result<OfferInfo> {
    let offer = PairOffer::decode(offer)?;
    Ok(OfferInfo {
        member: offer.member.clone(),
        network: offer.network.clone(),
        keeper: hex32(&offer.keeper.0),
        relays: offer.relays.clone(),
        expires_at: offer.expires_at,
    })
}

fn render(review: &DeviceReview, entry: &ReviewEntry, analysis: &Analysis, now: u64) -> Review {
    let status = match entry.vote {
        None => ReviewStatus::Pending,
        Some(Vote::Approved) => ReviewStatus::Approved,
        Some(Vote::Rejected) => ReviewStatus::Rejected,
    };
    let proposal = &review.proposal;
    Review {
        id: proposal.id().map(|id| hex32(&id)).unwrap_or_default(),
        vault: hex32(&review.vault),
        vault_name: review.vault_name.clone(),
        summary: proposal.summary.clone(),
        kind: analysis.kind,
        proposer: proposal.proposer,
        expires_at: proposal.expires_at,
        config_version: proposal.config_version,
        threshold: review.config.threshold,
        signers: review.config.signer_count,
        calls: analysis.calls.clone(),
        authwits: analysis.authwits.clone(),
        checks: analysis.checks.clone(),
        simulation: review.simulation.clone(),
        signable: analysis.passed()
            && status == ReviewStatus::Pending
            && now <= proposal.expires_at,
        status,
        outcome: entry.outcome.clone(),
    }
}

fn entry_id(entry: &ReviewEntry) -> Option<[u8; 32]> {
    entry.review().ok()?.proposal.id().ok()
}

fn hex32(bytes: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}

/// An id as `0x` and lowercase hex.
fn normalized(id: &str) -> String {
    let digits = id.trim().trim_start_matches("0x").to_ascii_lowercase();
    format!("0x{digits}")
}
