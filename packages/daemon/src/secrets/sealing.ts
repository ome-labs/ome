import { createCipheriv, createDecipheriv, randomBytes } from "node:crypto";

import { DaemonError } from "../errors.ts";

const VERSION = 0;

/**
 * Seals `plaintext` with a 32-byte key (AES-256-GCM, random nonce): version byte, nonce, tag, ciphertext. Used for
 * the daemon's own secrets at rest; threshold cryptography stays in Rust.
 */
export function seal(key: Uint8Array, plaintext: Uint8Array): Buffer {
  const nonce = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, nonce);
  const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  return Buffer.concat([Buffer.from([VERSION]), nonce, cipher.getAuthTag(), ciphertext]);
}

export function open(key: Uint8Array, sealed: Uint8Array): Buffer {
  if (sealed[0] !== VERSION || sealed.length < 29)
    throw new DaemonError("failed", "unknown sealed format");
  const decipher = createDecipheriv("aes-256-gcm", key, sealed.subarray(1, 13));
  decipher.setAuthTag(sealed.subarray(13, 29));
  try {
    return Buffer.concat([decipher.update(sealed.subarray(29)), decipher.final()]);
  } catch {
    throw new DaemonError("auth", "the key does not open this file");
  }
}
