import type { KeeperContact, Manifest } from "@ome-labs/api";

import { toApiPoint } from "../aztec/convert.ts";
import type { VaultDeployment } from "../vault/deployment.ts";

/** The protocol version every Ome v0 vault speaks. */
export const PROTOCOL_VERSION = 0;

/**
 * The committee manifest members register with their keepers after the DKG (`vault.register`): the vault's address
 * context, both thresholds, and per member its signing key, verification share and keeper. `keepers` are in member
 * order.
 */
export function vaultManifest(
  deployment: VaultDeployment,
  keepers: readonly KeeperContact[],
  epoch = 0,
): Manifest {
  const { setup } = deployment;
  if (keepers.length !== setup.signerKeys.length)
    throw new Error("One keeper per member is required");
  return {
    protocolVersion: PROTOCOL_VERSION,
    vault: deployment.address.toString(),
    epoch,
    threshold: setup.threshold,
    viewingThreshold: setup.viewingThreshold,
    preaddress: deployment.preaddress.toString(),
    groupKey: toApiPoint(setup.groupKey),
    members: keepers.map((keeper, i) => {
      const signerKey = setup.signerKeys[i];
      const verificationShare = setup.verificationShares[i];
      if (!signerKey || !verificationShare)
        throw new Error(`Member ${i + 1} is missing from the setup`);
      return {
        index: i + 1,
        signerKey: toApiPoint(signerKey),
        verificationShare: toApiPoint(verificationShare),
        identity: keeper.identity,
        transport: keeper.transport,
        addresses: [...keeper.addresses],
      };
    }),
  };
}
