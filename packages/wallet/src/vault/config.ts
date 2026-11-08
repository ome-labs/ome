import { Contract } from "@aztec-labs/aztec.js/contracts";
import type { Wallet } from "@aztec-labs/aztec.js/wallet";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { Point } from "@aztec-labs/foundation/curves/grumpkin";
import type { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { VaultConfig } from "@ome-labs/api";

import { vaultArtifact } from "./artifact.ts";

type Field = bigint | number;

interface RawConfig {
  threshold: Field;
  signer_count: Field;
  version: Field;
  signer_set_commitment: Field;
  viewing_threshold: Field;
  committee_commitment: Field;
}

interface RawSignerSet {
  keys: { x: Field; y: Field }[];
  count: Field;
}

/** A `get_*` utility of the vault, called through `wallet`. */
async function utility(wallet: Wallet, address: AztecAddress, name: string): Promise<unknown> {
  const method = Contract.at(address, vaultArtifact(), wallet).methods[name];
  if (!method) throw new Error(`OmeVault has no ${name}`);
  const { result } = await method().simulate({ from: address });
  return result;
}

/**
 * The vault's config note (`get_config`), read through `wallet`, whose PXE must hold the vault. This is what the
 * daemon hands its keeper (`vault.observeConfig`) and what proposals are checked against.
 */
export async function readVaultConfig(wallet: Wallet, address: AztecAddress): Promise<VaultConfig> {
  const raw = (await utility(wallet, address, "get_config")) as RawConfig;
  return {
    threshold: Number(raw.threshold),
    signerCount: Number(raw.signer_count),
    version: Number(raw.version),
    signerSetCommitment: new Fr(BigInt(raw.signer_set_commitment)).toString(),
    viewingThreshold: Number(raw.viewing_threshold),
    committeeCommitment: new Fr(BigInt(raw.committee_commitment)).toString(),
  };
}

/** The ordered signer keys, rebuilt by the vault's `get_signer_set` from its signer notes. */
export async function readSignerKeys(wallet: Wallet, address: AztecAddress): Promise<Point[]> {
  const raw = (await utility(wallet, address, "get_signer_set")) as RawSignerSet;
  return raw.keys
    .slice(0, Number(raw.count))
    .map(({ x, y }) => new Point(new Fr(BigInt(x)), new Fr(BigInt(y))));
}
