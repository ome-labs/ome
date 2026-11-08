//! Vault-founding lobbies. A founder's keeper hosts a lobby; co-founders join with an out-of-band [`Invite`], see the
//! roster ([`LobbyState`]) live, and receive the agreed [`LobbyPlan`] when the founder starts. Each member's daemon
//! then starts the DKG itself.
//!
//! Lobby messages are ordinary signed messages with header scope [`scope`] and epoch 0. The join proof is a protocol
//! rule and lives in `ome-protocol`.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{bounded, ceremony::addresses};
use crate::{
    Error, IdentityPublic, KeeperIdentity,
    codec::{canonical, from_canonical, to_canonical},
    limits::{MAX_INVITE_CHARS, MAX_MEMBERS, MAX_NAME_BYTES, MAX_SEALED_SECRETS_BYTES},
    types::{Bytes64, WireField, WirePoint},
    version::{self, VersionRange},
};

/// Prefix of an invite's string form.
pub const INVITE_PREFIX: &str = "ome-invite-v0:";

/// Prefix of a join request's string form.
pub const JOIN_REQUEST_PREFIX: &str = "ome-join-v0:";

/// Domain of a join request's signature.
const JOIN_REQUEST_DOMAIN: &str = "ome.join-request.v0";

/// The header scope of a lobby's messages: its id zero-padded to 32 bytes.
#[must_use]
pub fn scope(lobby: &[u8; 16]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(lobby);
    out
}

/// How to reach a keeper, and which protocol versions it speaks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeeperContact {
    /// Ed25519 identity.
    pub identity: IdentityPublic,
    /// X25519 transport key.
    pub transport: [u8; 32],
    /// Protocol versions the keeper speaks.
    pub versions: VersionRange,
    /// Network addresses.
    pub addresses: Vec<String>,
}

impl KeeperContact {
    fn validate(&self) -> Result<(), Error> {
        self.versions.validate()?;
        addresses(&self.addresses)
    }
}

/// A lobby member: display name, signing key and keeper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyMember {
    /// Display name.
    pub name: String,
    /// Grumpkin key the member will sign approvals with.
    pub signer_key: WirePoint,
    /// The member's keeper.
    pub keeper: KeeperContact,
}

impl LobbyMember {
    fn validate(&self) -> Result<(), Error> {
        name("member.name", &self.name)?;
        self.keeper.validate()
    }
}

/// The secret that keys an invite's join proof. Zeroized on drop.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct JoinSecret(pub [u8; 32]);

/// An invitation to a lobby, passed out of band (chat, QR code). Whoever holds it can join while the lobby is open.
/// `Debug` omits the secret.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    /// Lobby id.
    pub lobby: [u8; 16],
    /// Join secret.
    pub secret: JoinSecret,
    /// The host's keeper.
    pub host: KeeperContact,
    /// The host's display name.
    pub host_name: String,
    /// Vault name.
    pub name: String,
    /// Proposed spending threshold `k`.
    pub threshold: u8,
    /// Proposed viewing threshold `t_v`.
    pub viewing_threshold: u8,
    /// Network label set by the daemon (opaque to keepers).
    pub network: String,
}

impl core::fmt::Debug for Invite {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Invite")
            .field("lobby", &self.lobby)
            .field("host", &self.host)
            .field("host_name", &self.host_name)
            .field("name", &self.name)
            .field("threshold", &self.threshold)
            .field("viewing_threshold", &self.viewing_threshold)
            .field("network", &self.network)
            .finish_non_exhaustive()
    }
}

impl Invite {
    /// Structural bounds: names, the host contact and thresholds (`1 <= k <= 8`, `2 <= t_v <= 8`).
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] naming the field.
    pub fn validate(&self) -> Result<(), Error> {
        name("name", &self.name)?;
        name("host_name", &self.host_name)?;
        name("network", &self.network)?;
        self.host.validate()?;
        bounded("threshold", usize::from(self.threshold), 1, MAX_MEMBERS)?;
        bounded(
            "viewing_threshold",
            usize::from(self.viewing_threshold),
            2,
            MAX_MEMBERS,
        )
    }

    /// The string form: [`INVITE_PREFIX`] followed by base64url (no padding) of the canonical bytes.
    ///
    /// # Errors
    /// Bounds errors.
    pub fn encode(&self) -> Result<String, Error> {
        self.validate()?;
        let text = format!(
            "{INVITE_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(to_canonical(self, MAX_INVITE_CHARS)?)
        );
        if text.len() > MAX_INVITE_CHARS {
            return Err(Error::TooLarge(text.len()));
        }
        Ok(text)
    }

    /// Parses the string form; surrounding whitespace is ignored. Only the canonical form is accepted: base64 with
    /// non-zero trailing bits and non-canonical bytes are refused, so every invite has exactly one string form.
    ///
    /// # Errors
    /// [`Error::TooLarge`], [`Error::Malformed`] or bounds errors.
    pub fn decode(text: &str) -> Result<Self, Error> {
        let text = text.trim();
        if text.len() > MAX_INVITE_CHARS {
            return Err(Error::TooLarge(text.len()));
        }
        let body = text.strip_prefix(INVITE_PREFIX).ok_or(Error::Malformed)?;
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| Error::Malformed)?;
        let invite: Self = from_canonical(&bytes, MAX_INVITE_CHARS)?;
        invite.validate()?;
        Ok(invite)
    }
}

/// A newcomer's request to join an existing vault, passed out of band to a member who plans the rotation: its name,
/// signer key and keeper, signed by that keeper's identity so the keeper contact is the newcomer's own. The member
/// still confirms out of band that the request came from the person they mean to add.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRequest {
    /// The newcomer.
    pub member: LobbyMember,
    /// Ed25519 signature by `member.keeper.identity` over the domain and `member`.
    pub signature: Bytes64,
}

impl JoinRequest {
    /// Signs `member` with `identity`, which must be the member's keeper.
    ///
    /// # Errors
    /// [`Error::UnexpectedSigner`] for another keeper, or bounds errors.
    pub fn sign(identity: &KeeperIdentity, member: LobbyMember) -> Result<Self, Error> {
        if identity.public() != member.keeper.identity {
            return Err(Error::UnexpectedSigner);
        }
        member.validate()?;
        let signature = identity.sign(&canonical(&(JOIN_REQUEST_DOMAIN, &member))?);
        Ok(Self { member, signature })
    }

    /// Checks the bounds and that the member's keeper signed it.
    ///
    /// # Errors
    /// Bounds errors or [`Error::InvalidSignature`].
    pub fn verify(&self) -> Result<(), Error> {
        self.member.validate()?;
        self.member.keeper.identity.verify(
            &canonical(&(JOIN_REQUEST_DOMAIN, &self.member))?,
            &self.signature,
        )
    }

    /// The string form: [`JOIN_REQUEST_PREFIX`] followed by base64url (no padding) of the canonical bytes.
    ///
    /// # Errors
    /// Bounds errors or [`Error::TooLarge`].
    pub fn encode(&self) -> Result<String, Error> {
        self.verify()?;
        let text = format!(
            "{JOIN_REQUEST_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(to_canonical(self, MAX_INVITE_CHARS)?)
        );
        if text.len() > MAX_INVITE_CHARS {
            return Err(Error::TooLarge(text.len()));
        }
        Ok(text)
    }

    /// Parses and verifies the string form; surrounding whitespace is ignored.
    ///
    /// # Errors
    /// [`Error::TooLarge`], [`Error::Malformed`], bounds errors or [`Error::InvalidSignature`].
    pub fn decode(text: &str) -> Result<Self, Error> {
        let text = text.trim();
        if text.len() > MAX_INVITE_CHARS {
            return Err(Error::TooLarge(text.len()));
        }
        let body = text
            .strip_prefix(JOIN_REQUEST_PREFIX)
            .ok_or(Error::Malformed)?;
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| Error::Malformed)?;
        let request: Self = from_canonical(&bytes, MAX_INVITE_CHARS)?;
        request.verify()?;
        Ok(request)
    }
}

/// A guest asks to join, proving it holds the invite's secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyJoin {
    /// Lobby id.
    pub lobby: [u8; 16],
    /// Join proof, keyed by the invite's secret.
    pub proof: [u8; 32],
    /// The joiner; its keeper identity must be the message's signer.
    pub member: LobbyMember,
}

impl LobbyJoin {
    pub(super) fn validate(&self) -> Result<(), Error> {
        self.member.validate()
    }
}

/// The roster, sent by the host to every joined member whenever it changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyState {
    /// Lobby id.
    pub lobby: [u8; 16],
    /// Increases with every roster change; members apply only newer states.
    pub revision: u32,
    /// Vault name.
    pub name: String,
    /// Proposed spending threshold.
    pub threshold: u8,
    /// Proposed viewing threshold.
    pub viewing_threshold: u8,
    /// Members, host first.
    pub members: Vec<LobbyMember>,
    /// Whether the host started the lobby.
    pub started: bool,
}

impl LobbyState {
    pub(super) fn validate(&self) -> Result<(), Error> {
        name("name", &self.name)?;
        members(&self.members)
    }
}

impl SealedSecrets {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded(
            "ciphertext",
            self.ciphertext.len(),
            1,
            MAX_SEALED_SECRETS_BYTES,
        )?;
        bounded("index", usize::from(self.index), 1, MAX_MEMBERS)
    }
}

/// The vault's shared protocol secrets sealed for one member: in a lobby plan for each founder, or in a reshare for a
/// member who joins after founding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedSecrets {
    /// Recipient's 1-based index in the plan, or in the reshare's new committee.
    pub index: u8,
    /// HPKE encapsulated key.
    pub encapsulated_key: [u8; 32],
    /// AEAD ciphertext.
    pub ciphertext: Vec<u8>,
}

/// What the founders agreed: the protocol version, DKG session, thresholds, salt, members in slot order (host = slot 0)
/// and the founding secrets sealed to each member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyPlan {
    /// Lobby id.
    pub lobby: [u8; 16],
    /// Protocol version of the vault: one every member's keeper speaks (see [`version::negotiate`]).
    pub protocol_version: u16,
    /// DKG session id.
    pub session: [u8; 32],
    /// Spending threshold `k`.
    pub threshold: u8,
    /// Viewing threshold `t_v`.
    pub viewing_threshold: u8,
    /// Vault deployment salt.
    pub salt: WireField,
    /// Members in slot order; member `i` (1-based) is slot `i - 1`.
    pub members: Vec<LobbyMember>,
    /// Sealed founding secrets, one per member.
    pub secrets: Vec<SealedSecrets>,
}

impl LobbyPlan {
    /// The sealed secrets for member `index` (1-based).
    #[must_use]
    pub fn sealed_for(&self, index: u8) -> Option<&SealedSecrets> {
        self.secrets.iter().find(|s| s.index == index)
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        version::require_supported(self.protocol_version)?;
        members(&self.members)?;
        if self
            .members
            .iter()
            .any(|m| !m.keeper.versions.contains(self.protocol_version))
        {
            return Err(Error::OutOfBounds {
                field: "protocol_version",
            });
        }
        bounded("secrets", self.secrets.len(), 0, MAX_MEMBERS)?;
        for sealed in &self.secrets {
            bounded(
                "secrets.ciphertext",
                sealed.ciphertext.len(),
                1,
                MAX_SEALED_SECRETS_BYTES,
            )?;
        }
        Ok(())
    }
}

/// From a guest: it leaves an open lobby. From the host: the lobby is closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyLeave {
    /// Lobby id.
    pub lobby: [u8; 16],
}

fn name(field: &'static str, value: &str) -> Result<(), Error> {
    bounded(field, value.len(), 0, MAX_NAME_BYTES)
}

fn members(list: &[LobbyMember]) -> Result<(), Error> {
    bounded("members", list.len(), 1, MAX_MEMBERS)?;
    list.iter().try_for_each(LobbyMember::validate)
}
