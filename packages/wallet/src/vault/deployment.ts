import { Fr } from "@aztec-labs/foundation/curves/bn254";
import type { Point } from "@aztec-labs/foundation/curves/grumpkin";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import {
  CompleteAddress,
  type ContractInstanceWithAddress,
  computePartialAddress,
  getContractInstanceFromInstantiationParams,
} from "@aztec-labs/stdlib/contract";
import { addressContext, committeeCommitment, signerSetCommitment } from "@ome-labs/wasm";

import { toApiPoint } from "../aztec/convert.ts";
import { vaultArtifact } from "./artifact.ts";
import { type VaultProtocolSecrets, vaultPublicKeys } from "./keys.ts";

/** Slots in the contract's signer set and committee. */
export const MAX_SIGNERS = 8;

/** Everything that fixes a vault's address and initial config; agreed by all founding members. */
export interface VaultSetup {
  /** Signer keys in slot order; member `i` (1-based) is slot `i - 1`. */
  readonly signerKeys: readonly Point[];
  /** Spending threshold `k`. */
  readonly threshold: number;
  /** Viewing threshold `t_v`. */
  readonly viewingThreshold: number;
  /** Group incoming viewing key `Ivpk` from the DKG. */
  readonly groupKey: Point;
  /** Verification shares `Y_i` in member order. */
  readonly verificationShares: readonly Point[];
  readonly secrets: VaultProtocolSecrets;
  readonly salt: Fr;
}

/** A vault's derived instance: address, preaddress and the constructor call that initializes it. */
export interface VaultDeployment {
  readonly setup: VaultSetup;
  readonly instance: ContractInstanceWithAddress;
  readonly completeAddress: CompleteAddress;
  readonly address: AztecAddress;
  readonly preaddress: Fr;
  readonly partialAddress: Fr;
  readonly constructorArgs: unknown[];
  /** The contract's `signer_set_commitment` and `committee_commitment` for this setup. */
  readonly signerSetCommitment: Fr;
  readonly committeeCommitment: Fr;
}

const slots = (points: readonly Point[]) =>
  Array.from({ length: MAX_SIGNERS }, (_, i) => {
    const point = points[i];
    return point ? { x: point.x, y: point.y } : { x: Fr.ZERO, y: Fr.ZERO };
  });

function validate(setup: VaultSetup): void {
  const n = setup.signerKeys.length;
  if (n < 1 || n > MAX_SIGNERS) throw new Error(`A vault has 1..=${MAX_SIGNERS} signers, got ${n}`);
  if (setup.verificationShares.length !== n)
    throw new Error("One verification share per signer is required");
  if (setup.threshold < 1 || setup.threshold > n)
    throw new Error(`Threshold ${setup.threshold} out of 1..=${n}`);
  if (setup.viewingThreshold < 2 || setup.viewingThreshold > n) {
    throw new Error(`Viewing threshold ${setup.viewingThreshold} out of 2..=${n}`);
  }
}

/**
 * Derives the vault instance for `setup` (universal deployment, no deployer in the address) and checks that the
 * address Aztec derives matches the one the threshold crypto derives from `preaddress` and `Ivpk`.
 */
export async function deriveVault(setup: VaultSetup): Promise<VaultDeployment> {
  validate(setup);
  const publicKeys = await vaultPublicKeys(setup.secrets, setup.groupKey);
  const constructorArgs = [
    slots(setup.signerKeys),
    setup.signerKeys.length,
    setup.threshold,
    setup.viewingThreshold,
    slots(setup.verificationShares),
  ];
  const instance = await getContractInstanceFromInstantiationParams(vaultArtifact(), {
    constructorArtifact: "constructor",
    constructorArgs,
    salt: setup.salt,
    publicKeys,
    deployer: AztecAddress.ZERO,
  });
  const partialAddress = await computePartialAddress(instance);
  const completeAddress = await CompleteAddress.fromPublicKeysAndPartialAddress(
    publicKeys,
    partialAddress,
  );
  if (!completeAddress.address.equals(instance.address))
    throw new Error("Vault address derivation mismatch");
  const preaddress = await completeAddress.getPreaddress();
  const context = addressContext(preaddress.toString(), toApiPoint(setup.groupKey));
  if (context.address !== instance.address.toString()) {
    throw new Error("Vault address does not match the committee's address context");
  }
  return {
    setup,
    instance,
    completeAddress,
    address: instance.address,
    preaddress,
    partialAddress,
    constructorArgs,
    signerSetCommitment: Fr.fromString(signerSetCommitment(setup.signerKeys.map(toApiPoint))),
    committeeCommitment: Fr.fromString(
      committeeCommitment(setup.viewingThreshold, setup.verificationShares.map(toApiPoint)),
    ),
  };
}
