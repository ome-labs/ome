import { Fr } from "@aztec-labs/foundation/curves/bn254";
import type { Hex16, Hex32, Hex64, Point, VaultConfig } from "@ome-labs/api";
import { type VaultProtocolSecrets, type VaultSetup, fromApiPoint } from "@ome-labs/wallet";

/** The founding constructor of a vault: public among members, it fixes the vault's address. */
export interface DeploymentRecord {
  readonly salt: Hex32;
  readonly signerKeys: Point[];
  readonly threshold: number;
  readonly viewingThreshold: number;
  readonly groupKey: Point;
  readonly verificationShares: Point[];
}

/** What a daemon keeps about one of its member's vaults. Its protocol secrets are sealed separately. */
export interface VaultRecord {
  readonly address: Hex32;
  readonly name: string;
  readonly foundedAt: number;
  readonly founding: {
    readonly lobby: Hex16 | null;
    readonly session: Hex32;
    deployTx: Hex32 | null;
  };
  readonly deployment: DeploymentRecord;
  /** Display names by keeper identity. */
  names: Record<Hex32, string>;
  /** Each committee epoch's ceremony transcript, for its self-send tagging secret. */
  transcripts: Record<string, Hex64>;
  config: { config: VaultConfig; readAt: number; matchesCommittee: boolean } | null;
  /** `joining` until a rotation that adds this member activates. */
  status: "active" | "removed" | "joining";
}

/** The setup `deriveVault` takes, from a record and its secrets. */
export function setupOf(record: VaultRecord, secrets: VaultProtocolSecrets): VaultSetup {
  const { deployment } = record;
  return {
    signerKeys: deployment.signerKeys.map(fromApiPoint),
    threshold: deployment.threshold,
    viewingThreshold: deployment.viewingThreshold,
    groupKey: fromApiPoint(deployment.groupKey),
    verificationShares: deployment.verificationShares.map(fromApiPoint),
    secrets,
    salt: Fr.fromString(deployment.salt),
  };
}

export const secretsName = (vault: Hex32): string => `vault-${vault}`;
