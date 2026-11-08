// A consistent vault without a network: signers from `@ome-labs/wasm` sealed under platform keys (no Argon2id), a
// committee dealt from one secret, the deployment Aztec derives, and the manifest and config the contract would hold.
import { randomBytes } from "node:crypto";

import { Fr } from "@aztec-labs/foundation/curves/bn254";
import type { VaultConfig } from "@ome-labs/api";
import { UnlockedSigner, type VaultInput, generateSigner } from "@ome-labs/wasm";

import {
  type VaultDeployment,
  deriveVault,
  fromApiPoint,
  generateVaultProtocolSecrets,
  vaultManifest,
} from "../src/index.ts";
import { type TestCommittee, dealTestCommittee } from "../src/testing/index.ts";

const hex32 = (): string => `0x${randomBytes(32).toString("hex")}`;

export interface VaultFixture {
  readonly committee: TestCommittee;
  readonly signers: UnlockedSigner[];
  readonly deployment: VaultDeployment;
  readonly input: VaultInput;
}

export async function vaultFixture(
  members = 3,
  threshold = 2,
  viewingThreshold = 2,
): Promise<VaultFixture> {
  const committee = dealTestCommittee(viewingThreshold, members);
  const signers = Array.from({ length: members }, () => {
    const key = hex32();
    return UnlockedSigner.open(generateSigner({ key }), { key });
  });
  const deployment = await deriveVault({
    signerKeys: signers.map((signer) => fromApiPoint(signer.publicKey())),
    threshold,
    viewingThreshold,
    groupKey: committee.groupKey,
    verificationShares: committee.verificationShares,
    secrets: await generateVaultProtocolSecrets(),
    salt: Fr.random(),
  });
  const keepers = signers.map(() => ({ identity: hex32(), transport: hex32(), addresses: [] }));
  const config: VaultConfig = {
    threshold,
    signerCount: members,
    version: 0,
    signerSetCommitment: deployment.signerSetCommitment.toString(),
    viewingThreshold,
    committeeCommitment: deployment.committeeCommitment.toString(),
  };
  return {
    committee,
    signers,
    deployment,
    input: { manifest: vaultManifest(deployment, keepers), config },
  };
}
