//! Store-and-forward for members who are offline: frames a peer could not receive directly are sealed into that pair's
//! daily mailbox (`ome_crypto::sealing::mailbox`) at the relays, and this keeper polls its own mailboxes, its paired
//! devices' and its open pairing offers'.

use std::collections::HashMap;

use ome_crypto::sealing::{
    envelope::TransportPublic,
    mailbox::{self, mailbox_id},
};
use ome_net::{Channel, Inbound, Peer};
use ome_wire::{limits::MAX_FETCH_MAILBOXES, messages::mailbox::RelayEnvelope};

use crate::{
    Result, keeper::Keeper, random::os_rng, service::lobby::lobby_mailbox_senders,
    store::ManifestStatus,
};

const SECONDS_PER_DAY: u64 = 86_400;
/// Bound on fetch rounds per poll (each round deletes what it delivered).
const MAX_FETCH_ROUNDS: usize = 16;

impl Keeper {
    fn day(&self) -> u32 {
        u32::try_from(self.now() / SECONDS_PER_DAY).unwrap_or(u32::MAX)
    }

    /// Seals `frame` into `peer`'s mailbox at the relays (when the peer cannot be reached directly).
    pub(crate) async fn deposit(&self, peer: &Peer, frame: &[u8]) -> Result<()> {
        let Some(service) = self.0.mailbox.clone() else {
            return Ok(());
        };
        let day = self.day();
        let keys = &self.0.keys;
        let recipient = TransportPublic(peer.transport);
        let id = mailbox_id(
            &keys.transport,
            &recipient,
            &keys.transport_public,
            &recipient,
            day,
        )?;
        let sealed = mailbox::seal(&id, day, &keys.transport, &recipient, frame, &mut os_rng())?;
        let ttl = u64::from(self.0.config.mailbox_ttl_days) * SECONDS_PER_DAY;
        Ok(service
            .deposit(RelayEnvelope {
                recipient: id,
                expires_at: self.now() + ttl,
                sealed,
            })
            .await?)
    }

    /// Collects what other members left in this keeper's mailboxes, processes it like direct traffic and deletes it at
    /// the relays. Returns the number of envelopes delivered.
    ///
    /// # Errors
    /// Mailbox service errors.
    pub async fn poll_mailbox(&self) -> Result<usize> {
        let Some(service) = self.0.mailbox.clone() else {
            return Ok(0);
        };
        let mut delivered = self.poll_pairings().await?;
        let senders: Vec<[u8; 32]> = {
            let mut state = self.0.state.lock().await;
            state.last_mailbox_poll = self.now();
            let me = self.identity();
            let mut keys: Vec<[u8; 32]> = state
                .vaults
                .values()
                .filter(|v| v.status == ManifestStatus::Active)
                .flat_map(|v| v.committee.peers_except(&me))
                .map(|p| p.transport)
                .collect();
            keys.extend(lobby_mailbox_senders(&state, &me));
            keys.extend(Self::device_senders(&state));
            keys.sort_unstable();
            keys.dedup();
            keys
        };
        let keys = &self.0.keys;
        let today = self.day();
        let mut mailboxes: HashMap<[u8; 32], ([u8; 32], u32)> = HashMap::new();
        for sender in senders {
            let sender_public = TransportPublic(sender);
            for day in today.saturating_sub(self.0.config.mailbox_ttl_days)..=today {
                let id = mailbox_id(
                    &keys.transport,
                    &sender_public,
                    &sender_public,
                    &keys.transport_public,
                    day,
                )?;
                mailboxes.insert(id, (sender, day));
            }
        }
        let ids: Vec<[u8; 32]> = mailboxes.keys().copied().collect();
        for chunk in ids.chunks(MAX_FETCH_MAILBOXES) {
            for _ in 0..MAX_FETCH_ROUNDS {
                let envelopes = service.fetch(chunk.to_vec()).await?;
                if envelopes.is_empty() {
                    break;
                }
                let mut done = Vec::with_capacity(envelopes.len());
                for stored in envelopes {
                    done.push(stored.id);
                    let Some(&(sender, day)) = mailboxes.get(&stored.envelope.recipient) else {
                        continue;
                    };
                    let Ok(frame) = mailbox::open(
                        &stored.envelope.recipient,
                        day,
                        &keys.transport,
                        &TransportPublic(sender),
                        &stored.envelope.sealed,
                    ) else {
                        continue;
                    };
                    Box::pin(self.on_inbound(Inbound {
                        channel: Channel::Key(sender),
                        bytes: frame.to_vec(),
                    }))
                    .await;
                    delivered += 1;
                }
                service.delete(done).await?;
            }
        }
        Ok(delivered)
    }
}
