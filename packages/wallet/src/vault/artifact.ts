import { type ContractArtifact, loadContractArtifact } from "@aztec-labs/stdlib/abi";
import type { NoirCompiledContract } from "@aztec-labs/stdlib/noir";
import compiled from "@ome-labs/contracts/OmeVault.json" with { type: "json" };

let artifact: ContractArtifact | undefined;

/** The compiled `OmeVault` contract (built by `@ome-labs/contracts`). */
export function vaultArtifact(): ContractArtifact {
  artifact ??= loadContractArtifact(compiled as unknown as NoirCompiledContract);
  return artifact;
}
