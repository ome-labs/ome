/**
 * Ome wallet: hosts an Aztec PXE able to operate Ome vaults. The patched PXE routes every use of an account's incoming
 * viewing secret through {@link ThresholdKeyStore}, which never holds a vault's secret and obtains `address_secret · P`
 * from the members' keepers through threshold ECDH. See `architecture/pxe-integration.md`.
 */
export { fromApiPoint, fromHex32, toApiPoint, toHex32 } from "./aztec/convert.ts";
export { initBarretenberg } from "./barretenberg.ts";
export { KeeperClient, KeeperError } from "./keeper/client.ts";
export { PROTOCOL_VERSION, vaultManifest } from "./keeper/manifest.ts";
export { KeeperPartialsSource } from "./keeper/partials-source.ts";
export type { EcdhProvider, EcdhPurpose } from "./keys/ecdh.ts";
export {
  type PartialsRequest,
  type PartialsSource,
  ThresholdEcdhProvider,
} from "./keys/threshold-ecdh.ts";
export {
  type ThresholdAccountKeys,
  ThresholdKeyStore,
  ThresholdViewingError,
} from "./keys/threshold-key-store.ts";
export { VaultAccountContract, VaultWitnessProvider } from "./vault/account.ts";
export { vaultArtifact } from "./vault/artifact.ts";
export { readSignerKeys, readVaultConfig } from "./vault/config.ts";
export {
  MAX_SIGNERS,
  type VaultDeployment,
  type VaultSetup,
  deriveVault,
} from "./vault/deployment.ts";
export {
  type VaultProtocolSecrets,
  decodeVaultProtocolSecrets,
  encodeVaultProtocolSecrets,
  generateVaultProtocolSecrets,
  thresholdAccountKeys,
  vaultPublicKeys,
} from "./vault/keys.ts";
export {
  type PayloadContracts,
  fromVaultRequest,
  payloadContracts,
  toVaultRequest,
} from "./vault/payload.ts";
export {
  OmeWallet,
  type OmeWalletOptions,
  type ProposalTerms,
  type VaultTx,
  VaultTxError,
  type VaultViewing,
} from "./wallet/ome-wallet.ts";
export { AZTEC_VERSION } from "./version.ts";
