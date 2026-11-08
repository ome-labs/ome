// A consistent vault for the tests: signers sealed under platform keys (no Argon2id), a committee dealt from one
// secret, and the manifest and config the contract would hold for them.
import { randomBytes } from "node:crypto";

import type { Hex32 } from "@ome-labs/api";
import {
  type Chain,
  UnlockedSigner,
  type VaultInput,
  addressContext,
  committeeCommitment,
  generateSigner,
  signerSetCommitment,
} from "@ome-labs/wasm";
import { type DealtCommittee, dealShares } from "@ome-labs/wasm/testing";

/** `items[index]`, failing the test if it is missing. */
export function at<T>(items: readonly T[], index: number): T {
  const item = items[index];
  if (item === undefined) throw new Error(`no item ${index}`);
  return item;
}

/** 31 random bytes: below both field moduli, so valid as a field element or scalar. */
export const randomField = (): Hex32 => `0x00${randomBytes(31).toString("hex")}`;

export const chain: Chain = { chainId: randomField(), version: `0x${"00".repeat(31)}01` };

export interface Fixture {
  vault: VaultInput;
  signers: UnlockedSigner[];
  committee: DealtCommittee;
  secret: Hex32;
}

export function fixture(members = 3, threshold = 2, viewingThreshold = 2): Fixture {
  const signers = Array.from({ length: members }, () => {
    const key = randomField();
    return UnlockedSigner.open(generateSigner({ key }), { key });
  });
  const secret = randomField();
  const committee = dealShares(secret, viewingThreshold, members);
  const preaddress = randomField();
  const signerKeys = signers.map((signer) => signer.publicKey());
  return {
    signers,
    committee,
    secret,
    vault: {
      manifest: {
        protocolVersion: 0,
        vault: addressContext(preaddress, committee.groupKey).address,
        epoch: 0,
        threshold,
        viewingThreshold,
        preaddress,
        groupKey: committee.groupKey,
        members: signerKeys.map((signerKey, position) => ({
          index: position + 1,
          signerKey,
          verificationShare: at(committee.verificationShares, position),
          identity: `0x${randomBytes(32).toString("hex")}`,
          transport: `0x${randomBytes(32).toString("hex")}`,
          addresses: [],
        })),
      },
      config: {
        threshold,
        signerCount: members,
        version: 0,
        signerSetCommitment: signerSetCommitment(signerKeys),
        viewingThreshold,
        committeeCommitment: committeeCommitment(viewingThreshold, committee.verificationShares),
      },
    },
  };
}
