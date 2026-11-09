import { type ContractArtifact, loadContractArtifact } from "@aztec-labs/stdlib/abi";
import type { NoirCompiledContract } from "@aztec-labs/stdlib/noir";
import compiled from "@ome-labs/contracts/OmeVault.json" with { type: "json" };
import compiledPreview from "@ome-labs/contracts/OmeVaultPreview.json" with { type: "json" };

let artifact: ContractArtifact | undefined;
let preview: ContractArtifact | undefined;

/** The compiled `OmeVault` contract (built by `@ome-labs/contracts`). */
export function vaultArtifact(): ContractArtifact {
  artifact ??= loadContractArtifact(compiled as unknown as NoirCompiledContract);
  return artifact;
}

/**
 * The vault's simulation stand-in (`OmeVaultPreview`): the vault's storage and functions with an entrypoint that needs
 * no approvals. A proposal is previewed with the vault's class overridden by this one, so calls to the vault itself run
 * the vault's real checks. Never deployed.
 */
export function vaultPreviewArtifact(): ContractArtifact {
  preview ??= loadContractArtifact(compiledPreview as unknown as NoirCompiledContract);
  return preview;
}
