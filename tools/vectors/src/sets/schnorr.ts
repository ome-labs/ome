// Grumpkin Schnorr signatures as Aztec accounts and the vault contract use them, with the `[s.lo, s.hi, e.lo, e.hi]`
// limbs the auth witness carries.
//
// Signing draws a random nonce, so a fresh signature differs on every run. A committed signature is kept as long as it
// still verifies under the installed Aztec and its limbs still decode the same way; only then is the file stable
// across regenerations and `--check` meaningful.

import { Schnorr, SchnorrSignature } from "@aztec-labs/foundation/crypto/schnorr";
import { Fq, Fr } from "@aztec-labs/foundation/curves/bn254";
import type { Point } from "@aztec-labs/foundation/curves/grumpkin";

import { type EncodedPoint, hex, point } from "../encoding.ts";

const CASES: [bigint, bigint][] = [
  [0x11n, 0x2an],
  [0xfeedn, 0x1234567890n],
  [Fq.MODULUS - 5n, Fr.MODULUS - 1n],
  [0x5eedn * 0x9e3779b97f4a7c15n, 0x7ec9a2n],
];

export interface SchnorrCase {
  privateKey: string;
  publicKey: EncodedPoint;
  message: string;
  signature: string;
  limbs: string[];
  verifies: boolean;
  tamperedMessage: string;
  tamperedVerifies: boolean;
}

const schnorr = new Schnorr();

/** The committed signature for this key and message, if it is still valid. */
async function reusable(
  committed: SchnorrCase[] | undefined,
  privateKey: Fq,
  message: Fr,
  publicKey: Point,
): Promise<SchnorrSignature | undefined> {
  const entry = committed?.find(
    (c) => c.privateKey === hex(privateKey) && c.message === hex(message),
  );
  if (!entry) return undefined;

  const signature = new SchnorrSignature(Buffer.from(entry.signature.slice(2), "hex"));
  const verifies = await schnorr.verifySignature(message, publicKey, signature);
  const limbsMatch = signature.toLimbFields().map(hex).join() === entry.limbs.join();

  return verifies && limbsMatch ? signature : undefined;
}

export async function schnorrVectors(committed: SchnorrCase[] | undefined): Promise<SchnorrCase[]> {
  return Promise.all(
    CASES.map(async ([key, value]) => {
      const privateKey = new Fq(key);
      const message = new Fr(value);
      const publicKey = await schnorr.computePublicKey(privateKey);

      const signature =
        (await reusable(committed, privateKey, message, publicKey)) ??
        (await schnorr.constructSignature(message, privateKey));
      const tampered = new Fr((value + 1n) % Fr.MODULUS);

      return {
        privateKey: hex(privateKey),
        publicKey: point(publicKey),
        message: hex(message),
        signature: `0x${signature.toBuffer().toString("hex")}`,
        limbs: signature.toLimbFields().map(hex),
        verifies: await schnorr.verifySignature(message, publicKey, signature),
        tamperedMessage: hex(tampered),
        tamperedVerifies: await schnorr.verifySignature(tampered, publicKey, signature),
      };
    }),
  );
}
