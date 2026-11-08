import type { Account, AccountContract } from "@aztec-labs/aztec.js/account";
import { BaseAccount } from "@aztec-labs/aztec.js/account";
import { DefaultAccountEntrypoint } from "@aztec-labs/entrypoints/account";
import type { AuthWitnessProvider } from "@aztec-labs/entrypoints/interfaces";
import type { Fr } from "@aztec-labs/foundation/curves/bn254";
import type { ContractArtifact } from "@aztec-labs/stdlib/abi";
import { AuthWitness } from "@aztec-labs/stdlib/auth-witness";
import type { CompleteAddress } from "@aztec-labs/stdlib/contract";

import { vaultArtifact } from "./artifact.ts";
import type { VaultDeployment } from "./deployment.ts";

/**
 * Serves auth witnesses assembled from collected approvals. The vault never signs locally: a witness exists only for
 * message hashes whose approvals were provided, and anything else fails.
 */
export class VaultWitnessProvider implements AuthWitnessProvider {
  readonly #witnesses = new Map<string, Fr[]>();

  /** Makes `fields` the witness for `messageHash` until {@link revoke}. */
  provide(messageHash: Fr, fields: Fr[]): void {
    this.#witnesses.set(messageHash.toString(), fields);
  }

  revoke(messageHash: Fr): void {
    this.#witnesses.delete(messageHash.toString());
  }

  createAuthWit(messageHash: Fr): Promise<AuthWitness> {
    const fields = this.#witnesses.get(messageHash.toString());
    if (!fields)
      return Promise.reject(
        new Error(`No approvals collected for vault message ${messageHash.toString()}`),
      );
    return Promise.resolve(new AuthWitness(messageHash, fields));
  }
}

/** The Ome vault as an Aztec account contract (default entrypoint, k-of-n witness). */
export class VaultAccountContract implements AccountContract {
  readonly #deployment: VaultDeployment;
  readonly witnesses = new VaultWitnessProvider();

  constructor(deployment: VaultDeployment) {
    this.#deployment = deployment;
  }

  getContractArtifact(): Promise<ContractArtifact> {
    return Promise.resolve(vaultArtifact());
  }

  getInitializationFunctionAndArgs(): Promise<{
    constructorName: string;
    constructorArgs: unknown[];
  }> {
    return Promise.resolve({
      constructorName: "constructor",
      constructorArgs: this.#deployment.constructorArgs,
    });
  }

  getImmutablesHash(): Promise<Fr | undefined> {
    return Promise.resolve(undefined);
  }

  getAuthWitnessProvider(_address: CompleteAddress): AuthWitnessProvider {
    return this.witnesses;
  }

  getAccount(completeAddress: CompleteAddress): Account {
    return new BaseAccount(
      new DefaultAccountEntrypoint(completeAddress.address, this.witnesses),
      this.witnesses,
      completeAddress,
    );
  }
}
