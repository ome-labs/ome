import type { Hex64 } from "@ome-labs/api";

import { DaemonError } from "../errors.ts";

/** Version byte of {@link encodeHandoff}. */
const VERSION = 1;
/** The keeper's limit on sealed secrets. */
const MAX_BYTES = 1024;
/** `encodeVaultProtocolSecrets`' length. */
const PROTOCOL_SECRETS_BYTES = 225;

/**
 * What a dealer hands a joining member in the reshare, sealed to the joiner's keeper: the vault's protocol secrets,
 * and the ceremony transcript of every earlier committee epoch, from which the joiner derives those epochs' self-send
 * tagging secrets and finds the notes the vault sent itself then (its change, for example). A member removed earlier
 * never receives these, only members who join.
 *
 * Layout: version, protocol secrets, epoch count, then per epoch its number (u32, big-endian) and 64-byte transcript.
 * The keeper's 1,024-byte limit fits 12 earlier epochs.
 */
export function encodeHandoff(
  protocolSecrets: Uint8Array,
  transcripts: Record<string, Hex64>,
): Buffer {
  const epochs = Object.entries(transcripts).toSorted(([a], [b]) => Number(a) - Number(b));
  const parts = [
    Buffer.from([VERSION]),
    Buffer.from(protocolSecrets),
    Buffer.from([epochs.length]),
  ];
  for (const [epoch, transcript] of epochs) {
    const number = Buffer.alloc(4);
    number.writeUInt32BE(Number(epoch));
    parts.push(number, Buffer.from(transcript.slice(2), "hex"));
  }
  const handoff = Buffer.concat(parts);
  if (handoff.length > MAX_BYTES)
    throw new DaemonError(
      "invalid",
      "this vault has too many earlier epochs to hand a joiner (at most 12)",
    );
  return handoff;
}

export function decodeHandoff(bytes: Uint8Array): {
  protocolSecrets: Buffer;
  transcripts: Record<string, Hex64>;
} {
  const data = Buffer.from(bytes);
  if (data[0] !== VERSION || data.length < PROTOCOL_SECRETS_BYTES + 2)
    throw new DaemonError("invalid", "not a v1 joiner hand-off");
  const protocolSecrets = data.subarray(1, 1 + PROTOCOL_SECRETS_BYTES);
  const count = data[1 + PROTOCOL_SECRETS_BYTES] ?? 0;
  const transcripts: Record<string, Hex64> = {};
  let offset = 2 + PROTOCOL_SECRETS_BYTES;
  for (let i = 0; i < count; i += 1) {
    if (offset + 68 > data.length) throw new DaemonError("invalid", "a truncated joiner hand-off");
    transcripts[String(data.readUInt32BE(offset))] =
      `0x${data.subarray(offset + 4, offset + 68).toString("hex")}`;
    offset += 68;
  }
  return { protocolSecrets, transcripts };
}
