// The OmeVault contract's own hashes, computed with Aztec's Poseidon2 and Grumpkin: signer-set and committee
// commitments and the approval message under the `ome.*.v0` domain tags. The Noir contract pins the same values in
// `contracts/ome_vault/src/test/unit/wire_vectors.nr`; ome-protocol must reproduce them too.

import { poseidon2HashWithSeparator } from "@aztec-labs/foundation/crypto/poseidon";
import { Fq, Fr } from "@aztec-labs/foundation/curves/bn254";
import { derivePublicKeyFromSecretKey } from "@aztec-labs/stdlib/keys";

import { hex } from "../encoding.mjs";

const MAX_SIGNERS = 8;

/** A domain tag as the contract writes it: the field whose big-endian bytes spell the ASCII name. */
const tag = (name) => BigInt(`0x${Buffer.from(name).toString("hex")}`);

const TAGS = {
  approval: "ome.approval.v0",
  signers: "ome.signers.v0",
  committee: "ome.committee.v0",
};

/** `count` points `(seed + i) · G`, zero-padded to `MAX_SIGNERS`, as the contract tests derive them. */
async function distinctPoints(count, seed) {
  return Promise.all(
    Array.from({ length: MAX_SIGNERS }, async (_, i) => {
      if (i >= count) return { x: hex(Fr.ZERO), y: hex(Fr.ZERO) };

      const p = await derivePublicKeyFromSecretKey(new Fq(BigInt(seed + i)));
      return { x: hex(p.x), y: hex(p.y) };
    }),
  );
}

const coordinates = (points) => points.flatMap(({ x, y }) => [BigInt(x), BigInt(y)]);

const hash = async (inputs, name) =>
  hex(
    await poseidon2HashWithSeparator(
      inputs.map((v) => new Fr(v)),
      tag(name),
    ),
  );

async function signerSet(count, seed) {
  const keys = await distinctPoints(count, seed);
  return { count, keys, commitment: await hash(coordinates(keys), TAGS.signers) };
}

async function committee(viewingThreshold, memberCount, seed) {
  const shares = await distinctPoints(memberCount, seed);
  return {
    viewingThreshold,
    memberCount,
    shares,
    commitment: await hash([BigInt(viewingThreshold), ...coordinates(shares)], TAGS.committee),
  };
}

async function approvalMessage(vault, outerHash, configVersion, expiresAt) {
  return {
    vault: hex(new Fr(vault)),
    outerHash: hex(new Fr(outerHash)),
    configVersion,
    expiresAt: expiresAt.toString(),
    message: await hash([vault, outerHash, BigInt(configVersion), expiresAt], TAGS.approval),
  };
}

export async function contractVectors() {
  return {
    tags: Object.fromEntries(
      Object.entries(TAGS).map(([key, name]) => [key, { name, field: hex(new Fr(tag(name))) }]),
    ),
    signerSets: [await signerSet(3, 1), await signerSet(MAX_SIGNERS, 1)],
    committees: [await committee(2, 3, 1000), await committee(MAX_SIGNERS, MAX_SIGNERS, 1000)],
    approvalMessages: [
      await approvalMessage(0x1234n, 0x5678n, 3, 1_700_000_000n),
      await approvalMessage(0x1234n, 0x5678n, 0xffff_ffff, 0xffff_ffff_ffff_ffffn),
    ],
  };
}
