//! The device's link to its member's keeper: a short-lived libp2p client node (no listener, no circuit) that dials
//! the relays, deposits sealed frames in the pair's mailbox and fetches what the keeper left. See
//! `architecture/mobile.md`.

use std::{sync::Arc, time::Duration};

use ome_crypto::sealing::{
    envelope::TransportPublic,
    mailbox::{self, mailbox_id},
    pairing::{self, pairing_mailbox},
};
use ome_net::{Libp2pConfig, Libp2pTransport, Mailbox, Multiaddr, RelayMailbox};
use ome_wire::{
    Header, IdentityPublic, Message, SignedMessage, decode, encode,
    messages::{device::PairOffer, mailbox::RelayEnvelope},
    version::PROTOCOL_VERSION,
};

use crate::{FfiError, error::Result, keys::DeviceKeys, keys::rng};

const SECONDS_PER_DAY: u64 = 86_400;
/// Days an envelope waits at the relays, as the keeper's mailbox lifetime.
pub(crate) const MAILBOX_DAYS: u32 = 7;

/// The keeper on the other end.
#[derive(Clone, Debug)]
pub(crate) struct Peer {
    pub(crate) identity: IdentityPublic,
    pub(crate) transport: TransportPublic,
    pub(crate) relays: Vec<String>,
}

/// An open link: a running client node and its relays' mailboxes. Stopped when dropped.
pub(crate) struct Link {
    node: Arc<Libp2pTransport>,
    mailbox: RelayMailbox,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.node.stop();
    }
}

impl Link {
    /// Starts a client node that reaches `relays`.
    pub(crate) async fn open(keys: &DeviceKeys, relays: &[String]) -> Result<Self> {
        let relays = relays
            .iter()
            .map(|r| {
                r.parse::<Multiaddr>()
                    .map_err(|e| FfiError::invalid(format!("relay {r}: {e}")))
            })
            .collect::<Result<Vec<_>>>()?;
        let config = Libp2pConfig {
            listen: Vec::new(),
            relays,
            external: Vec::new(),
            request_timeout: Duration::from_secs(15),
            ready_timeout: Duration::from_secs(5),
            reserve: false,
        };
        let (node, _inbound) = Libp2pTransport::start(&keys.identity, config).await?;
        Ok(Self {
            mailbox: RelayMailbox::new(Arc::clone(&node)),
            node,
        })
    }

    /// Deposits a pairing request into the offer's pairing mailbox.
    pub(crate) async fn deposit_pairing(&self, offer: &PairOffer, frame: &[u8]) -> Result<()> {
        let sealed = pairing::seal(&offer.secret.0, frame, &mut rng())?;
        self.mailbox
            .deposit(RelayEnvelope {
                recipient: pairing_mailbox(&offer.secret.0),
                expires_at: offer.expires_at,
                sealed,
            })
            .await?;
        Ok(())
    }

    /// Seals `frame` into today's mailbox from this device to `peer`.
    pub(crate) async fn send(&self, keys: &DeviceKeys, peer: &Peer, frame: &[u8]) -> Result<()> {
        let now = unix_now();
        let day = day(now);
        let mine = keys.transport_public();
        let id = mailbox_id(
            &keys.transport,
            &peer.transport,
            &mine,
            &peer.transport,
            day,
        )?;
        let sealed = mailbox::seal(
            &id,
            day,
            &keys.transport,
            &peer.transport,
            frame,
            &mut rng(),
        )?;
        self.mailbox
            .deposit(RelayEnvelope {
                recipient: id,
                expires_at: now + u64::from(MAILBOX_DAYS) * SECONDS_PER_DAY,
                sealed,
            })
            .await?;
        Ok(())
    }

    /// Everything `peer` left for this device in the mailbox window, opened and verified to be signed by `peer`, and
    /// deleted at the relays.
    pub(crate) async fn receive(
        &self,
        keys: &DeviceKeys,
        peer: &Peer,
    ) -> Result<Vec<SignedMessage>> {
        let today = day(unix_now());
        let mine = keys.transport_public();
        let mut boxes = Vec::new();
        for day in today.saturating_sub(MAILBOX_DAYS)..=today {
            boxes.push((
                mailbox_id(
                    &keys.transport,
                    &peer.transport,
                    &peer.transport,
                    &mine,
                    day,
                )?,
                day,
            ));
        }
        let envelopes = self
            .mailbox
            .fetch(boxes.iter().map(|(id, _)| *id).collect())
            .await?;
        let mut messages = Vec::new();
        let mut done = Vec::with_capacity(envelopes.len());
        for stored in envelopes {
            done.push(stored.id);
            let Some(&(id, day)) = boxes
                .iter()
                .find(|(id, _)| *id == stored.envelope.recipient)
            else {
                continue;
            };
            let Ok(frame) = mailbox::open(
                &id,
                day,
                &keys.transport,
                &peer.transport,
                &stored.envelope.sealed,
            ) else {
                continue;
            };
            let Ok(message) = decode(&frame) else {
                continue;
            };
            if message.verify_from(&peer.identity).is_ok() {
                messages.push(message);
            }
        }
        if !done.is_empty() {
            self.mailbox.delete(done).await?;
        }
        messages.sort_by_key(|m| m.header.created_at);
        Ok(messages)
    }
}

/// Signs `body` as the device.
pub(crate) fn sign(
    keys: &DeviceKeys,
    scope: [u8; 32],
    epoch: u32,
    body: Message,
) -> Result<Vec<u8>> {
    let mut id = [0u8; 16];
    rand_core::Rng::fill_bytes(&mut rng(), &mut id);
    let header = Header {
        scope,
        epoch,
        id,
        created_at: unix_now(),
    };
    Ok(encode(&SignedMessage::sign(
        &keys.identity,
        PROTOCOL_VERSION,
        header,
        body,
    )?)?)
}

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn day(now: u64) -> u32 {
    u32::try_from(now / SECONDS_PER_DAY).unwrap_or(u32::MAX)
}
