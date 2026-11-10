//! Every size limit of the wire format, in one place so the relay, the keeper and the TypeScript side agree.
//!
//! Decoding checks the total length first and every count and length after parsing, so no field can grow past its
//! limit. Changing a limit is a wire-format change.

/// Largest encoded [`crate::SignedMessage`] or message body.
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;

/// Most extensions on one record.
pub const MAX_EXTENSIONS: usize = 16;
/// Largest extension.
pub const MAX_EXTENSION_BYTES: usize = 4 * 1024;

/// Most members of a vault: the contract's signer limit.
pub const MAX_MEMBERS: usize = 8;
/// Most network addresses per keeper.
pub const MAX_ADDRESSES: usize = 8;
/// Longest network address.
pub const MAX_ADDRESS_BYTES: usize = 256;

/// Largest sealed share ciphertext: a 32-byte scalar plus the AEAD tag, with headroom.
pub const MAX_SEALED_SHARE_BYTES: usize = 256;
/// Largest forwarded frame in an echo.
pub const MAX_ECHO_BYTES: usize = 64 * 1024;

/// Most points in one ECDH request (a sync batch).
pub const MAX_ECDH_POINTS: usize = 4096;

/// Largest proposal payload: the opaque Aztec execution request, sent apart from the proposal.
pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024;
/// Most payloads asked for in one request.
pub const MAX_PAYLOAD_REQUESTS: usize = 64;
/// Largest artifact chunk. An artifact (gzip-compressed JSON) is split into chunks this size.
pub const MAX_ARTIFACT_CHUNK_BYTES: usize = 256 * 1024;
/// Most chunks of one artifact: 32 MiB compressed.
pub const MAX_ARTIFACT_CHUNKS: usize = 128;
/// Most chunks asked for in one request.
pub const MAX_ARTIFACT_REQUEST: usize = 16;
/// Most private authwits one proposal grants alongside its transaction.
pub const MAX_AUTHWITS: usize = 4;
/// Longest human-readable proposal summary.
pub const MAX_SUMMARY_BYTES: usize = 4 * 1024;
/// Longest rejection reason.
pub const MAX_REASON_BYTES: usize = 280;
/// Most records of each kind in one sync request.
pub const MAX_SYNC_ITEMS: usize = 4096;

/// Longest lobby, member or network name.
pub const MAX_NAME_BYTES: usize = 64;
/// Longest invite string.
pub const MAX_INVITE_CHARS: usize = 4096;
/// Largest protocol-secrets blob a founder seals to each member.
pub const MAX_LOBBY_SECRETS_BYTES: usize = 1024;
/// Largest sealed secrets ciphertext: the blob plus the AEAD tag.
pub const MAX_SEALED_SECRETS_BYTES: usize = MAX_LOBBY_SECRETS_BYTES + 16;

/// Largest sealed payload a relay stores.
pub const MAX_ENVELOPE_BYTES: usize = 512 * 1024;
/// Most mailboxes in one fetch.
pub const MAX_FETCH_MAILBOXES: usize = 128;
/// Most envelopes in one fetch answer.
pub const MAX_FETCH_ENVELOPES: usize = 1024;
/// Most envelope ids in one delete.
pub const MAX_DELETE_IDS: usize = 1024;
/// Largest mailbox message (a fetch answer carries several envelopes).
pub const MAX_MAILBOX_MESSAGE_BYTES: usize = 4 << 20;
