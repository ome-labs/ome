//! Stream protocols and framing.
//!
//! Each conversation has its own libp2p stream protocol (a lane), so requests are prioritized, timed out and admitted
//! separately. The trailing number of a protocol name is the `ome-wire` protocol version: a keeper that speaks several
//! versions offers each name, and libp2p negotiates the one both peers speak.
//!
//! Requests and responses are length-prefixed byte strings (`u32` big-endian). The length is checked before the body
//! is read, so an oversized frame is refused without allocating it. Keeper lanes carry one encoded `SignedMessage` per
//! request and answer with an empty acknowledgement; the mailbox lane carries mailbox messages.

use std::io;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::{StreamProtocol, request_response::Codec};
use ome_wire::{
    Message, decode,
    limits::{MAX_MAILBOX_MESSAGE_BYTES, MAX_MESSAGE_BYTES},
};

/// Threshold ECDH requests and responses.
pub const ECDH: StreamProtocol = StreamProtocol::new("/ome/ecdh/0");
/// Manifests, DKG and reshare rounds, complaints, aborts and echoes.
pub const CEREMONY: StreamProtocol = StreamProtocol::new("/ome/ceremony/0");
/// Proposals, payloads, votes, receipts and sync requests.
pub const SYNC: StreamProtocol = StreamProtocol::new("/ome/sync/0");
/// Vault-founding lobbies: the only lane on which a keeper hosting an open lobby accepts frames from strangers.
pub const LOBBY: StreamProtocol = StreamProtocol::new("/ome/lobby/0");
/// Keeper to relay mailboxes.
pub const MAILBOX: StreamProtocol = StreamProtocol::new("/ome/mailbox/0");
/// The identify protocol version of Ome nodes.
pub const IDENTIFY_VERSION: &str = "/ome/0";
/// Largest acknowledgement on a keeper lane.
const MAX_ACK_BYTES: usize = 64;

/// The conversation a frame belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lane {
    /// Threshold ECDH.
    Ecdh,
    /// Ceremonies and manifests.
    Ceremony,
    /// Spending and sync.
    Sync,
    /// Founding lobbies.
    Lobby,
}

impl Lane {
    /// The lane of an encoded signed message. Frames that do not decode go to the ceremony lane, where the receiver
    /// rejects them like any other invalid frame.
    #[must_use]
    pub fn of(frame: &[u8]) -> Self {
        match decode(frame).map(|message| message.body) {
            Ok(Message::EcdhRequest(_) | Message::EcdhResponse(_)) => Self::Ecdh,
            Ok(
                Message::Proposal(_)
                | Message::PayloadRequest(_)
                | Message::Payload(_)
                | Message::Approval(_)
                | Message::Rejection(_)
                | Message::ExecutionReceipt(_)
                | Message::ExecutionClaim(_)
                | Message::SyncRequest(_)
                | Message::ArtifactRequest(_)
                | Message::ArtifactManifest(_)
                | Message::ArtifactChunk(_),
            ) => Self::Sync,
            Ok(
                Message::LobbyJoin(_)
                | Message::LobbyState(_)
                | Message::LobbyPlan(_)
                | Message::LobbyLeave(_),
            ) => Self::Lobby,
            _ => Self::Ceremony,
        }
    }
}

/// Length-prefixed codec with per-direction size limits.
#[derive(Clone, Copy, Debug)]
pub struct FrameCodec {
    max_request: usize,
    max_response: usize,
}

impl FrameCodec {
    /// Keeper lanes: one signed message per request, an empty acknowledgement back.
    #[must_use]
    pub const fn keeper() -> Self {
        Self {
            max_request: MAX_MESSAGE_BYTES,
            max_response: MAX_ACK_BYTES,
        }
    }

    /// The mailbox lane.
    #[must_use]
    pub const fn mailbox() -> Self {
        Self {
            max_request: MAX_MAILBOX_MESSAGE_BYTES,
            max_response: MAX_MAILBOX_MESSAGE_BYTES,
        }
    }
}

/// Reads one frame of at most `max` bytes. The length prefix is checked before the body is allocated.
///
/// # Errors
/// [`io::ErrorKind::InvalidData`] for an oversized frame, [`io::ErrorKind::UnexpectedEof`] for a truncated one.
pub async fn read_frame<T: AsyncRead + Unpin + Send>(
    io: &mut T,
    max: usize,
) -> io::Result<Vec<u8>> {
    let mut length = [0u8; 4];
    io.read_exact(&mut length).await?;
    let length =
        usize::try_from(u32::from_be_bytes(length)).map_err(|_| io::ErrorKind::InvalidData)?;
    if length > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut bytes = vec![0u8; length];
    io.read_exact(&mut bytes).await?;
    Ok(bytes)
}

/// Writes one frame of at most `max` bytes and closes the stream.
///
/// # Errors
/// [`io::ErrorKind::InvalidInput`] for an oversized frame, or write errors.
pub async fn write_frame<T: AsyncWrite + Unpin + Send>(
    io: &mut T,
    bytes: &[u8],
    max: usize,
) -> io::Result<()> {
    if bytes.len() > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame too large",
        ));
    }
    let length = u32::try_from(bytes.len()).map_err(|_| io::ErrorKind::InvalidInput)?;
    io.write_all(&length.to_be_bytes()).await?;
    io.write_all(bytes).await?;
    io.close().await
}

impl Codec for FrameCodec {
    type Protocol = StreamProtocol;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    async fn read_request<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Vec<u8>> {
        read_frame(io, self.max_request).await
    }

    async fn read_response<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
    ) -> io::Result<Vec<u8>> {
        read_frame(io, self.max_response).await
    }

    async fn write_request<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
        request: Vec<u8>,
    ) -> io::Result<()> {
        write_frame(io, &request, self.max_request).await
    }

    async fn write_response<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &StreamProtocol,
        io: &mut T,
        response: Vec<u8>,
    ) -> io::Result<()> {
        write_frame(io, &response, self.max_response).await
    }
}
