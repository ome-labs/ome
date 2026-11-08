import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { GrumpkinScalar, Point } from "@aztec-labs/foundation/curves/grumpkin";
import {
  PublicKeys,
  deriveKeys,
  derivePublicKeyFromSecretKey,
  hashPublicKey,
} from "@aztec-labs/stdlib/keys";

import type { ThresholdAccountKeys } from "../keys/threshold-key-store.ts";

/**
 * Account secrets every member holds for a vault, besides its threshold-shared incoming viewing key: the nullifier
 * hiding, outgoing viewing and tagging secrets, and the public message-signing and fallback keys (whose secrets are
 * discarded at generation). The founder generates them and the lobby hands them to every member sealed; they are never
 * published.
 */
export interface VaultProtocolSecrets {
  readonly nullifierHidingSecretKey: GrumpkinScalar;
  readonly outgoingViewingSecretKey: GrumpkinScalar;
  readonly taggingSecretKey: GrumpkinScalar;
  readonly messageSigningPublicKey: Point;
  readonly fallbackPublicKey: Point;
}

/** Fresh protocol secrets for a new vault. */
export async function generateVaultProtocolSecrets(): Promise<VaultProtocolSecrets> {
  const keys = await deriveKeys(Fr.random());
  return {
    nullifierHidingSecretKey: keys.masterNullifierHidingSecretKey,
    outgoingViewingSecretKey: keys.masterOutgoingViewingSecretKey,
    taggingSecretKey: keys.masterTaggingSecretKey,
    messageSigningPublicKey: keys.masterMessageSigningPublicKey,
    fallbackPublicKey: keys.masterFallbackPublicKey,
  };
}

/** Version byte of {@link encodeVaultProtocolSecrets}. */
const SECRETS_VERSION = 0;
/** Version byte, three scalars and two points of two fields each. */
const SECRETS_BYTES = 1 + 7 * 32;

/**
 * The bytes the lobby seals to each member (`lobby.start`'s `secrets`): a version byte, then the three secret scalars
 * and the two public points, 32 bytes per value.
 */
export function encodeVaultProtocolSecrets(secrets: VaultProtocolSecrets): Uint8Array {
  return Buffer.concat([
    Buffer.from([SECRETS_VERSION]),
    secrets.nullifierHidingSecretKey.toBuffer(),
    secrets.outgoingViewingSecretKey.toBuffer(),
    secrets.taggingSecretKey.toBuffer(),
    secrets.messageSigningPublicKey.x.toBuffer(),
    secrets.messageSigningPublicKey.y.toBuffer(),
    secrets.fallbackPublicKey.x.toBuffer(),
    secrets.fallbackPublicKey.y.toBuffer(),
  ]);
}

/** Reads {@link encodeVaultProtocolSecrets}' bytes, checking the points are on the curve. */
export function decodeVaultProtocolSecrets(bytes: Uint8Array): VaultProtocolSecrets {
  if (bytes.length !== SECRETS_BYTES || bytes[0] !== SECRETS_VERSION)
    throw new Error("Not a v0 vault protocol secrets encoding");
  const value = (i: number) => Buffer.from(bytes.subarray(1 + i * 32, 1 + (i + 1) * 32));
  const point = (i: number) => {
    const p = new Point(Fr.fromBuffer(value(i)), Fr.fromBuffer(value(i + 1)));
    if (!p.isOnCurve()) throw new Error("Vault protocol secrets carry an invalid point");
    return p;
  };
  return {
    nullifierHidingSecretKey: GrumpkinScalar.fromBuffer(value(0)),
    outgoingViewingSecretKey: GrumpkinScalar.fromBuffer(value(1)),
    taggingSecretKey: GrumpkinScalar.fromBuffer(value(2)),
    messageSigningPublicKey: point(3),
    fallbackPublicKey: point(5),
  };
}

/** The vault's public keys: its protocol keys plus the committee's group key `Ivpk`. */
export async function vaultPublicKeys(
  secrets: VaultProtocolSecrets,
  groupKey: Point,
): Promise<PublicKeys> {
  return new PublicKeys(
    await hashPublicKey(await derivePublicKeyFromSecretKey(secrets.nullifierHidingSecretKey)),
    groupKey,
    await hashPublicKey(await derivePublicKeyFromSecretKey(secrets.outgoingViewingSecretKey)),
    await hashPublicKey(await derivePublicKeyFromSecretKey(secrets.taggingSecretKey)),
    await hashPublicKey(secrets.messageSigningPublicKey),
    await hashPublicKey(secrets.fallbackPublicKey),
  );
}

/** Key-store registration for the vault (no incoming viewing secret). */
export function thresholdAccountKeys(
  secrets: VaultProtocolSecrets,
  groupKey: Point,
): ThresholdAccountKeys {
  return {
    kind: "threshold",
    masterNullifierHidingSecretKey: secrets.nullifierHidingSecretKey,
    masterIncomingViewingPublicKey: groupKey,
    masterOutgoingViewingSecretKey: secrets.outgoingViewingSecretKey,
    masterTaggingSecretKey: secrets.taggingSecretKey,
    masterMessageSigningPublicKey: secrets.messageSigningPublicKey,
    masterFallbackPublicKey: secrets.fallbackPublicKey,
  };
}
