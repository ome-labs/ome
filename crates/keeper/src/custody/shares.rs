use ome_crypto::{
    SecretScalar, curve,
    threshold::{dkg::KeyShare, shamir::Commitments},
};
use ome_wire::types::{Bytes64, WirePoint, parse_points};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::SecretStore;
use crate::{Error, Result};

/// Where a viewing share lives in the secret store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareSlot {
    /// Result of a finished DKG, waiting for its vault to be registered.
    Dkg([u8; 32]),
    /// Result of a finished reshare, waiting for on-chain activation.
    Pending([u8; 32]),
    /// The share serving `vault` at committee `epoch`.
    Active([u8; 32], u32),
}

impl ShareSlot {
    pub(crate) fn name(self) -> String {
        match self {
            Self::Dkg(session) => format!("dkg-{}", hex::encode(session)),
            Self::Pending(session) => format!("pending-{}", hex::encode(session)),
            Self::Active(vault, epoch) => format!("share-{}-{epoch}", hex::encode(vault)),
        }
    }
}

/// Serialized form of a [`KeyShare`]; zeroized on drop.
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct StoredShare {
    index: u32,
    threshold: u32,
    participants: Vec<u32>,
    share: [u8; 32],
    #[zeroize(skip)]
    commitments: Vec<WirePoint>,
    #[zeroize(skip)]
    transcript: Bytes64,
}

impl StoredShare {
    fn from_key_share(share: &KeyShare) -> Result<Self> {
        Ok(Self {
            index: share.index(),
            threshold: u32::try_from(share.threshold())
                .map_err(|_| Error::Invalid("threshold".into()))?,
            participants: share.participants().to_vec(),
            share: curve::scalar_to_bytes(share.share().expose()),
            commitments: share
                .group_commitments()
                .points()
                .iter()
                .map(WirePoint::from)
                .collect(),
            transcript: Bytes64::from_array(share.transcript()),
        })
    }

    fn into_key_share(self) -> Result<KeyShare> {
        let share = SecretScalar::new(curve::scalar_from_bytes(&self.share)?);
        Ok(KeyShare::restore(
            self.index,
            usize::try_from(self.threshold).map_err(|_| Error::Invalid("threshold".into()))?,
            self.participants.clone(),
            share,
            Commitments::new(parse_points(&self.commitments)?)?,
            self.transcript.to_array(),
        )?)
    }

    /// Seals `share` into `slot`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn save(secrets: &SecretStore, slot: ShareSlot, share: &KeyShare) -> Result<()> {
        let stored = Self::from_key_share(share)?;
        let bytes = Zeroizing::new(
            postcard::to_allocvec(&stored).map_err(|e| Error::Storage(e.to_string()))?,
        );
        secrets.put(&slot.name(), &bytes)
    }

    /// Loads and validates the share in `slot`.
    ///
    /// # Errors
    /// [`Error::MissingSecret`], unseal errors, or [`Error::Crypto`] if the stored share no longer matches its
    /// commitments.
    pub fn load(secrets: &SecretStore, slot: ShareSlot) -> Result<KeyShare> {
        let bytes = secrets.get(&slot.name())?;
        let stored: Self = postcard::from_bytes(&bytes).map_err(|_| Error::Unseal(slot.name()))?;
        stored.into_key_share()
    }
}
