// A daemon context without a network or processes: a consistent vault from real `@ome-labs/wasm` values, an in-memory
// keeper that stores what the daemon puts, and a wallet whose chain-facing calls the test controls.
import { randomBytes } from "node:crypto";

import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { Manifest, VaultConfig } from "@ome-labs/api";
import {
  type VaultDeployment,
  deriveVault,
  fromApiPoint,
  generateVaultProtocolSecrets,
  vaultManifest,
} from "@ome-labs/wallet";
import { dealTestCommittee } from "@ome-labs/wallet/testing";
import { UnlockedSigner, generateSigner } from "@ome-labs/wasm";

import type { Context } from "../src/context.ts";
import type { VaultRecord } from "../src/vaults/records.ts";

const hex32 = (): string => `0x${randomBytes(32).toString("hex")}`;

export const CHAIN = { chainId: new Fr(31_337n).toString(), version: new Fr(1n).toString() };
export const NOW = 1_900_000_000;

export interface Fake {
  readonly context: Context;
  readonly signers: UnlockedSigner[];
  readonly deployment: VaultDeployment;
  readonly manifest: Manifest;
  readonly config: VaultConfig;
  readonly stored: { proposals: string[]; payloads: Map<string, string> };
  /** What `wallet.vaultTxOf` does; tests replace it. */
  vaultTxOf: (proposal: string, payload: string) => Promise<unknown>;
}

export async function fakeContext(members = 3, viewingThreshold = 2): Promise<Fake> {
  const committee = dealTestCommittee(viewingThreshold, members);
  const signers = Array.from({ length: members }, () => {
    const key = hex32();
    return UnlockedSigner.open(generateSigner({ key }), { key });
  });
  const deployment = await deriveVault({
    signerKeys: signers.map((s) => fromApiPoint(s.publicKey())),
    threshold: 2,
    viewingThreshold,
    groupKey: committee.groupKey,
    verificationShares: committee.verificationShares,
    secrets: await generateVaultProtocolSecrets(),
    salt: Fr.random(),
  });
  const keepers = signers.map(() => ({ identity: hex32(), transport: hex32(), addresses: [] }));
  const manifest = vaultManifest(deployment, keepers);
  const config: VaultConfig = {
    threshold: 2,
    signerCount: members,
    version: 0,
    signerSetCommitment: deployment.signerSetCommitment.toString(),
    viewingThreshold,
    committeeCommitment: deployment.committeeCommitment.toString(),
  };
  const address = deployment.address.toString();
  const record: VaultRecord = {
    address,
    name: "Treasury",
    foundedAt: 0,
    founding: { lobby: null, session: hex32(), deployTx: null },
    deployment: {
      salt: deployment.setup.salt.toString(),
      signerKeys: signers.map((s) => s.publicKey()),
      threshold: 2,
      viewingThreshold,
      groupKey: manifest.groupKey,
      verificationShares: manifest.members.map((m) => m.verificationShare),
    },
    names: {},
    transcripts: {},
    config: { config, readAt: NOW, matchesCommittee: true },
    status: "active",
  };
  const stored = { proposals: [] as string[], payloads: new Map<string, string>() };
  const fake: Fake = {
    signers,
    deployment,
    manifest,
    config,
    stored,
    vaultTxOf: () => Promise.reject(new Error("not set")),
    context: undefined as unknown as Context,
  };
  const keeper = {
    call: (method: string, params: Record<string, string>) => {
      switch (method) {
        case "proposals.list":
          return Promise.resolve(stored.proposals);
        case "payloads.get":
          return Promise.resolve(stored.payloads.get(params.hash ?? "") ?? null);
        case "approvals.list":
        case "rejections.list":
        case "receipts.list":
        case "claims.list":
          return Promise.resolve([]);
        default:
          return Promise.reject(new Error(`unexpected keeper call ${method}`));
      }
    },
  };
  const wallet = {
    getVaultConfig: () => Promise.resolve(config),
    vaultInput: () => Promise.resolve({ manifest, config }),
    chain: () => Promise.resolve(CHAIN),
    chainTime: () => Promise.resolve(NOW),
    vaultTxOf: (_vault: unknown, proposal: string, payload: string) =>
      fake.vaultTxOf(proposal, payload),
  };
  (fake as { context: Context }).context = {
    keeper: { client: keeper, state: { identity: keepers[0]?.identity } },
    vaults: {
      record: () => record,
      manifest: () => manifest,
      address: () => AztecAddress.fromStringUnsafe(address),
      myIndex: () => 1,
      ensureCurrent: () => Promise.resolve(),
      observe: () => Promise.resolve("current"),
    },
    state: { file: { tokens: {}, profile: { name: "Ada" } } },
    names: () => Promise.resolve(new Map()),
    wallet: () => wallet,
    emit: () => undefined,
  } as unknown as Context;
  return fake;
}
