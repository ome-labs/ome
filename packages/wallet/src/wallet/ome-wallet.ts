import { SchnorrInitializerlessAccountContract } from "@aztec-labs/accounts/schnorr";
import {
  StubSchnorrAccountContractArtifact,
  createStubSchnorrAccount,
} from "@aztec-labs/accounts/schnorr/stub";
import type { Account } from "@aztec-labs/aztec.js/account";
import { NO_FROM, type NoFrom } from "@aztec-labs/aztec.js/account";
import {
  Contract,
  ContractFunctionInteraction,
  type InteractionWaitOptions,
  type SendReturn,
} from "@aztec-labs/aztec.js/contracts";
import type {
  Aliased,
  ExecuteUtilityOptions,
  SendOptions,
  SimulateOptions,
  TxSimulationResultWithAppOffset,
} from "@aztec-labs/aztec.js/wallet";
import { AccountManager } from "@aztec-labs/aztec.js/wallet";
import {
  AccountFeePaymentMethodOptions,
  DefaultAccountEntrypoint,
} from "@aztec-labs/entrypoints/account";
import { poseidon2Hash } from "@aztec-labs/foundation/crypto/poseidon";
import { Schnorr } from "@aztec-labs/foundation/crypto/schnorr";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import type { GrumpkinScalar } from "@aztec-labs/foundation/curves/grumpkin";
import { type Logger, createLogger } from "@aztec-labs/foundation/log";
import type { AccountPrivacyKeys } from "@aztec-labs/key-store";
import {
  DEFAULT_TAGGING_SECRET_STRATEGY,
  type PXE,
  type PXEConfig,
  type ResolveTaggingSecretStrategy,
  type TaggingSecretStrategy,
  type TaggingSecretStrategyRequest,
  createPXE,
  getPXEConfig,
} from "@aztec-labs/pxe/server";
import type { FunctionCall } from "@aztec-labs/stdlib/abi";
import { AuthWitness } from "@aztec-labs/stdlib/auth-witness";
import type { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { getContractClassFromArtifact } from "@aztec-labs/stdlib/contract";
import type { GasSettings } from "@aztec-labs/stdlib/gas";
import type { AztecNode } from "@aztec-labs/stdlib/interfaces/client";
import { AppTaggingSecretKind } from "@aztec-labs/stdlib/logs";
import {
  type ExecutionPayload,
  SimulationOverrides,
  type TxExecutionRequest,
  type TxReceipt,
  type TxSimulationResult,
  type UtilityExecutionResult,
} from "@aztec-labs/stdlib/tx";
import { BaseWallet, type FeeOptions } from "@aztec-labs/wallet-sdk/base-wallet";
import type { Hex64, HexBytes, Manifest, VaultConfig } from "@ome-labs/api";
import {
  type Chain,
  type TallyResult,
  type VaultInput,
  approvalMessage,
  buildProposal,
  checkProposal,
  decodeProposal,
  epochTaggingSecret,
  tally,
} from "@ome-labs/wasm";

import { fromApiPoint } from "../aztec/convert.ts";
import { type PartialsSource, ThresholdEcdhProvider } from "../keys/threshold-ecdh.ts";
import { ThresholdKeyStore } from "../keys/threshold-key-store.ts";
import { VaultAccountContract } from "../vault/account.ts";
import { vaultArtifact } from "../vault/artifact.ts";
import { readVaultConfig } from "../vault/config.ts";
import type { VaultDeployment } from "../vault/deployment.ts";
import { thresholdAccountKeys } from "../vault/keys.ts";
import { fromVaultRequest, toVaultRequest } from "../vault/payload.ts";

/** Options for {@link OmeWallet.create}. */
export interface OmeWalletOptions {
  /** PXE config overrides. Without `dataDirectory` the PXE runs on a temporary store. */
  readonly pxe?: Partial<PXEConfig>;
  /** Tagging strategy for everything except a vault's unconstrained self-sends. Defaults to Aztec's default. */
  readonly taggingStrategy?: ResolveTaggingSecretStrategy;
  readonly logger?: Logger;
}

/** How this member's PXE reads a vault in the current committee epoch. */
export interface VaultViewing {
  /** The committee manifest the member's keeper serves (`vault.status`): epoch, thresholds, verification shares. */
  readonly manifest: Manifest;
  /**
   * The transcript of the epoch's ceremony (the founding DKG, then each reshare). The vault's self-sends are tagged
   * with a secret derived from it, which members removed earlier cannot derive.
   */
  readonly transcript: Hex64;
  /** Where partials come from: the member's keeper. */
  readonly source: PartialsSource;
  /** Called with members whose partials failed verification. */
  readonly onInvalidMembers?: (members: number[]) => void;
}

/** What a proposal's proposer states besides the calls. */
export interface ProposalTerms {
  /** Approval deadline, Unix seconds of chain time. */
  readonly expiresAt: number;
  /** Shown to the members. */
  readonly summary: string;
  /** Proposing member (1-based). */
  readonly proposer: number;
  /** Entrypoint nonce; reusing one cancels the other proposal. Random by default. */
  readonly txNonce?: Fr;
}

/** A vault transaction and the proposal that describes it. */
export interface VaultTx {
  readonly vault: AztecAddress;
  /** The calls (including any fee payment calls) and the PXE's execution data. */
  readonly payload: ExecutionPayload;
  readonly txNonce: Fr;
  /** Hash the entrypoint asks the witness for. */
  readonly outerHash: Fr;
  readonly configVersion: number;
  readonly expiresAt: number;
  /** Proposal id. */
  readonly id: string;
  /** The proposal and payload as the keeper stores them (`proposals.put`, `payloads.put`). */
  readonly proposal: HexBytes;
  readonly payloadBody: HexBytes;
  /** What each signer signs. */
  readonly approvalMessage: string;
}

/** Why a vault transaction did not run. */
export class VaultTxError extends Error {
  readonly reason: "outer-hash" | "not-approved" | "stale-config";

  constructor(reason: VaultTxError["reason"], message: string) {
    super(message);
    this.name = "VaultTxError";
    this.reason = reason;
  }
}

interface VaultRecord {
  readonly deployment: VaultDeployment;
  readonly contract: VaultAccountContract;
  readonly account: Account;
  manifest: Manifest;
  epochTaggingSecret: ReturnType<typeof fromApiPoint>;
}

/**
 * Aztec wallet hosting a patched PXE whose {@link ThresholdKeyStore} operates Ome vaults without their incoming
 * viewing secret. It holds a member's own fee-paying account and the member's vaults. A vault transaction is a
 * proposal: {@link prepareVaultTx} builds it, members approve it with their signers, and {@link executeVaultTx}
 * tallies the approvals and sends it. All protocol checks run in `@ome-labs/wasm`.
 */
export class OmeWallet extends BaseWallet {
  readonly keyStore: ThresholdKeyStore;
  readonly #accounts = new Map<string, Account>();
  readonly #vaults = new Map<string, VaultRecord>();
  readonly #pinnedNonces = new Map<string, Fr>();
  readonly #taggingStrategy: ResolveTaggingSecretStrategy | undefined;
  #stubClassId: Fr | undefined;

  private constructor(
    pxe: PXE,
    node: AztecNode,
    keyStore: ThresholdKeyStore,
    taggingStrategy: ResolveTaggingSecretStrategy | undefined,
    log: Logger,
  ) {
    super(pxe, node, log);
    this.keyStore = keyStore;
    this.#taggingStrategy = taggingStrategy;
  }

  /** Creates the PXE with the threshold key store and the vault tagging hook. */
  static async create(node: AztecNode, options: OmeWalletOptions = {}): Promise<OmeWallet> {
    const log = options.logger ?? createLogger("ome:wallet");
    const holder: { wallet?: OmeWallet } = {};
    let keyStore: ThresholdKeyStore | undefined;
    const pxe = await createPXE(
      node,
      { ...getPXEConfig(), autoSync: false, ...options.pxe },
      {
        createKeyStore: (store) => {
          keyStore = new ThresholdKeyStore(store);
          return keyStore;
        },
        hooks: {
          resolveTaggingSecretStrategy: (request) => {
            if (!holder.wallet) return Promise.resolve(DEFAULT_TAGGING_SECRET_STRATEGY);
            return holder.wallet.#resolveTaggingStrategy(request);
          },
        },
        loggers: {
          pxe: log.createChild("pxe"),
          store: log.createChild("pxe:data"),
          prover: log.createChild("prover"),
        },
      },
    );
    if (!keyStore)
      throw new Error(
        "The PXE did not use the injected key store; is the @aztec-labs/pxe patch applied?",
      );
    const wallet = new OmeWallet(pxe, node, keyStore, options.taggingStrategy, log);
    holder.wallet = wallet;
    return wallet;
  }

  /** Stops the PXE's job queue. */
  stop(): Promise<void> {
    return this.pxe.stop();
  }

  /** The PXE (for tests and a host's sync loop). */
  get pxeService(): PXE {
    return this.pxe;
  }

  override getAccounts(): Promise<Aliased<AztecAddress>[]> {
    return Promise.resolve([
      ...[...this.#vaults.values()].map((v) => ({ alias: "vault", item: v.deployment.address })),
      ...[...this.#accounts.values()].map((a) => ({ alias: "account", item: a.getAddress() })),
    ]);
  }

  protected override getAccountFromAddress(address: AztecAddress): Promise<Account> {
    const account =
      this.#vaults.get(address.toString())?.account ?? this.#accounts.get(address.toString());
    if (!account)
      return Promise.reject(new Error(`Account ${address.toString()} is not held by this wallet`));
    return Promise.resolve(account);
  }

  /** Adds a member's own Schnorr account (initializerless, as genesis-funded test accounts are). */
  async addSchnorrAccount(secret: Fr, salt: Fr, signingKey: GrumpkinScalar): Promise<AztecAddress> {
    const contract = new SchnorrInitializerlessAccountContract(signingKey);
    const publicKey = await new Schnorr().computePublicKey(signingKey);
    const immutablesHash = await poseidon2Hash([publicKey.x, publicKey.y]);
    const manager = await AccountManager.create(this, secret, contract, { salt, immutablesHash });
    const instance = manager.getInstance();
    if (!(await this.pxe.getContractInstance(instance.address))) {
      const artifact = await contract.getContractArtifact();
      await this.registerContract(instance, artifact, secret);
      const constructor = artifact.functions.find((f) => f.name === "constructor");
      if (!constructor) throw new Error("Schnorr initializerless artifact has no constructor");
      await new ContractFunctionInteraction(this, instance.address, constructor, [
        publicKey.x,
        publicKey.y,
      ]).simulate({ from: instance.address });
    }
    const account = await manager.getAccount();
    this.#accounts.set(account.getAddress().toString(), account);
    return account.getAddress();
  }

  /**
   * Registers a vault in this PXE: its instance, its keys without the incoming viewing secret, and the committee
   * that answers its ECDH requests.
   */
  async registerVault(deployment: VaultDeployment, viewing: VaultViewing): Promise<AztecAddress> {
    const { address, instance, partialAddress, setup } = deployment;
    if (viewing.manifest.vault !== address.toString())
      throw new Error("The manifest names another vault");
    await this.registerContract(instance, vaultArtifact());
    const keys = thresholdAccountKeys(setup.secrets, setup.groupKey);
    // The key store recognises threshold keys; PXE's type only names ordinary ones.
    const registered = await this.pxe.registerAccount(
      keys as unknown as AccountPrivacyKeys,
      partialAddress,
    );
    if (!registered.address.equals(address))
      throw new Error("Registered vault address does not match its instance");
    const contract = new VaultAccountContract(deployment);
    const record: VaultRecord = {
      deployment,
      contract,
      account: contract.getAccount(deployment.completeAddress),
      manifest: viewing.manifest,
      epochTaggingSecret: fromApiPoint(
        epochTaggingSecret(address.toString(), viewing.manifest.epoch, viewing.transcript),
      ),
    };
    this.#vaults.set(address.toString(), record);
    await this.setViewing(address, viewing);
    return address;
  }

  /**
   * Points a vault at the committee of a new epoch, after a rotation activated: its ECDH goes to that committee, and
   * its self-sends are tagged with the epoch's secret. Secrets of earlier epochs stay registered, so notes sent then
   * are still found.
   */
  async setViewing(vault: AztecAddress, viewing: VaultViewing): Promise<void> {
    const record = this.#vault(vault);
    record.manifest = viewing.manifest;
    this.keyStore.setEcdhProvider(
      vault,
      new ThresholdEcdhProvider(vault, viewing.manifest, viewing.source, viewing.onInvalidMembers),
    );
    record.epochTaggingSecret = fromApiPoint(
      epochTaggingSecret(vault.toString(), viewing.manifest.epoch, viewing.transcript),
    );
    await this.pxe.registerTaggingSecretSource({
      kind: "arbitrary-secret",
      recipient: vault,
      secret: record.epochTaggingSecret,
    });
  }

  /**
   * Registers the self-send tagging secret of an earlier committee epoch, so notes the vault sent itself then are found.
   * A member who joins later receives those epochs' transcripts in the reshare.
   */
  async addEpochSecret(vault: AztecAddress, epoch: number, transcript: Hex64): Promise<void> {
    this.#vault(vault);
    await this.pxe.registerTaggingSecretSource({
      kind: "arbitrary-secret",
      recipient: vault,
      secret: fromApiPoint(epochTaggingSecret(vault.toString(), epoch, transcript)),
    });
  }

  /**
   * Initializes a registered vault: `from` (a member's own account) sends the constructor with the vault in scope. The
   * vault needs no class or instance publication (it has no public functions). Returns the mined receipt.
   */
  async deployVault(
    vault: AztecAddress,
    options: {
      from: AztecAddress;
      fee?: Parameters<ContractFunctionInteraction["send"]>[0]["fee"];
    },
  ): Promise<TxReceipt> {
    const { deployment } = this.#vault(vault);
    const deploy = Contract.deploy(
      this,
      vaultArtifact(),
      deployment.constructorArgs,
      "constructor",
      {
        salt: deployment.setup.salt,
        publicKeys: deployment.completeAddress.publicKeys,
        universalDeploy: true,
      },
    );
    const { receipt } = await deploy.send({
      from: options.from,
      fee: options.fee,
      skipClassPublication: true,
      skipInstancePublication: true,
      skipRegistration: true,
      additionalScopes: [vault],
    });
    return receipt;
  }

  /** The vault's config note as stored on chain, read through this PXE. */
  async getVaultConfig(vault: AztecAddress): Promise<VaultConfig> {
    this.#vault(vault);
    await this.pxe.sync();
    return readVaultConfig(this, vault);
  }

  /**
   * The vault as the bindings check proposals against it: the manifest this wallet holds and the on-chain config.
   * Throws (in `@ome-labs/wasm`) when they disagree.
   */
  async vaultInput(vault: AztecAddress): Promise<VaultInput> {
    return { manifest: this.#vault(vault).manifest, config: await this.getVaultConfig(vault) };
  }

  /** The chain a vault transaction is bound to. */
  async chain(): Promise<Chain> {
    const info = await this.getChainInfo();
    return { chainId: info.chainId.toString(), version: info.version.toString() };
  }

  /** Latest block timestamp: the clock deadlines are checked against. */
  async chainTime(): Promise<number> {
    const block = await this.aztecNode.getBlockData("latest");
    if (!block) throw new Error("No latest block");
    return Number(block.header.globalVariables.timestamp);
  }

  /**
   * Builds the proposal for a vault transaction under the vault's current config. The outer hash is computed twice,
   * by `@ome-labs/wasm` from the calls and by Aztec's own entrypoint code, and must agree.
   */
  async prepareVaultTx(
    vault: AztecAddress,
    payload: ExecutionPayload,
    terms: ProposalTerms,
  ): Promise<VaultTx> {
    const input = await this.vaultInput(vault);
    const txNonce = terms.txNonce ?? Fr.random();
    const request = toVaultRequest(payload);
    const built = buildProposal(input, await this.chain(), {
      ...request,
      txNonce: txNonce.toString(),
      expiresAt: terms.expiresAt,
      summary: terms.summary,
      proposer: terms.proposer,
    });
    const outerHash = await this.#checkOuterHash(vault, payload, txNonce, built.outerHash);
    return {
      vault,
      payload,
      txNonce,
      outerHash,
      configVersion: input.config.version,
      expiresAt: terms.expiresAt,
      id: built.id,
      proposal: built.proposal,
      payloadBody: built.payload,
      approvalMessage: built.approvalMessage,
    };
  }

  /**
   * The vault transaction a received proposal describes, after `@ome-labs/wasm`'s checks (the payload is the one
   * referenced and hashes to the outer hash on this vault and chain, the config is current, the deadline has not
   * passed, fees stay off the vault) and Aztec's own outer hash for the rebuilt payload.
   */
  async vaultTxOf(
    vault: AztecAddress,
    proposal: HexBytes,
    payloadBody: HexBytes,
  ): Promise<VaultTx> {
    const input = await this.vaultInput(vault);
    const request = checkProposal(
      input,
      await this.chain(),
      proposal,
      payloadBody,
      await this.chainTime(),
    );
    const info = decodeProposal(proposal);
    const payload = fromVaultRequest(request);
    const txNonce = Fr.fromString(info.txNonce);
    const outerHash = await this.#checkOuterHash(vault, payload, txNonce, info.outerHash);
    return {
      vault,
      payload,
      txNonce,
      outerHash,
      configVersion: info.configVersion,
      expiresAt: info.expiresAt,
      id: info.id,
      proposal,
      payloadBody,
      approvalMessage: approvalMessage(
        vault.toString(),
        info.outerHash,
        info.configVersion,
        info.expiresAt,
      ),
    };
  }

  /** Counts the approvals and rejections on a vault transaction at chain time. */
  async tallyVaultTx(
    tx: VaultTx,
    approvals: readonly HexBytes[],
    rejections: readonly HexBytes[] = [],
  ): Promise<TallyResult> {
    return tally(
      await this.vaultInput(tx.vault),
      tx.proposal,
      [...approvals],
      [...rejections],
      await this.chainTime(),
    );
  }

  /**
   * Simulates a vault transaction before approvals exist, with the vault's class swapped for Aztec's stub account
   * (whose authorization always passes). Shows the effects of app calls; calls into the vault itself are not
   * previewable this way.
   */
  async previewVaultTx(tx: VaultTx): Promise<TxSimulationResult> {
    const { deployment } = this.#vault(tx.vault);
    await this.pxe.sync();
    const stubClassId = await this.#registerStubClass();
    const instance = await this.pxe.getContractInstance(tx.vault);
    if (!instance) throw new Error("Vault instance is not registered");
    const overrides = new SimulationOverrides({
      contracts: {
        [tx.vault.toString()]: { instance: { ...instance, currentContractClassId: stubClassId } },
      },
    });
    const feeOptions = await this.completeFeeOptions({
      from: tx.vault,
      feePayer: tx.payload.feePayer,
      forEstimation: true,
    });
    const request = await createStubSchnorrAccount(
      deployment.completeAddress,
    ).createTxExecutionRequest(
      tx.payload,
      feeOptions.gasSettings,
      await this.getChainInfo(),
      this.#entrypointOptions(tx.txNonce, feeOptions),
    );
    return this.pxe.simulateTx(request, {
      simulatePublic: true,
      overrides,
      scopes: [tx.vault],
      senderForTags: tx.vault,
    });
  }

  /**
   * Tallies `approvals`, and once `k` members approved under the vault's current config, assembles the witness,
   * proves and sends the transaction and waits for it to be mined. Approvals collected before a config change are
   * refused here; the contract would refuse them too.
   */
  async executeVaultTx(
    tx: VaultTx,
    approvals: readonly HexBytes[],
    options: { gasSettings?: Partial<GasSettings> } = {},
  ): Promise<TxReceipt> {
    const record = this.#vault(tx.vault);
    const { version } = await this.getVaultConfig(tx.vault);
    if (tx.configVersion !== version)
      throw new VaultTxError(
        "stale-config",
        `Approved under config version ${tx.configVersion}; the vault is at ${version}`,
      );
    const result = await this.tallyVaultTx(tx, approvals);
    if (result.status !== "approved" || !result.witness) {
      const refused = result.refused.map((r) => `#${r.index}: ${r.reason}`).join("; ");
      throw new VaultTxError(
        "not-approved",
        `Proposal is ${result.status} with ${result.approvals} of ${result.needed} approvals${refused ? ` (refused ${refused})` : ""}`,
      );
    }
    record.contract.witnesses.provide(
      tx.outerHash,
      result.witness.map((field) => Fr.fromString(field)),
    );
    this.#pinnedNonces.set(tx.vault.toString(), tx.txNonce);
    try {
      const { receipt } = await this.sendTx(tx.payload, {
        from: tx.vault,
        fee: options.gasSettings ? { gasSettings: options.gasSettings } : undefined,
      });
      return receipt;
    } finally {
      this.#pinnedNonces.delete(tx.vault.toString());
      record.contract.witnesses.revoke(tx.outerHash);
    }
  }

  // The PXE runs with auto-sync off; every operation syncs first so it anchors on the latest block.

  override async simulateTx(
    executionPayload: ExecutionPayload,
    opts: SimulateOptions,
  ): Promise<TxSimulationResultWithAppOffset> {
    await this.pxe.sync();
    return super.simulateTx(executionPayload, opts);
  }

  override async executeUtility(
    call: FunctionCall,
    opts: ExecuteUtilityOptions,
  ): Promise<UtilityExecutionResult> {
    await this.pxe.sync();
    return super.executeUtility(call, opts);
  }

  override async sendTx<W extends InteractionWaitOptions = undefined>(
    executionPayload: ExecutionPayload,
    opts: SendOptions<W>,
  ): Promise<SendReturn<W>> {
    await this.pxe.sync();
    return super.sendTx(executionPayload, opts);
  }

  /** Vault requests use the pinned proposal nonce and are always cancellable (cancellation burns the nonce). */
  protected override async createTxExecutionRequestFromPayloadAndFee(
    executionPayload: ExecutionPayload,
    from: AztecAddress | NoFrom,
    feeOptions: FeeOptions,
  ): Promise<TxExecutionRequest> {
    if (from === NO_FROM || !this.#vaults.has(from.toString())) {
      return super.createTxExecutionRequestFromPayloadAndFee(executionPayload, from, feeOptions);
    }
    const txNonce = this.#pinnedNonces.get(from.toString());
    if (!txNonce)
      throw new Error("Vault transactions go through prepareVaultTx and executeVaultTx");
    const account = await this.getAccountFromAddress(from);
    return account.createTxExecutionRequest(
      executionPayload,
      feeOptions.gasSettings,
      await this.getChainInfo(),
      this.#entrypointOptions(txNonce, feeOptions),
    );
  }

  /** Fees are never paid by the vault: its transactions always name an external fee payer. */
  #entrypointOptions(txNonce: Fr, feeOptions: FeeOptions) {
    const method =
      feeOptions.accountFeePaymentMethodOptions ?? AccountFeePaymentMethodOptions.EXTERNAL;
    if (method !== AccountFeePaymentMethodOptions.EXTERNAL)
      throw new Error("A vault does not pay its own fees; use an external fee payer");
    return { txNonce, cancellable: true, feePaymentMethodOptions: method };
  }

  /** Aztec's entrypoint hash for `payload`, which must equal the outer hash the proposal carries. */
  async #checkOuterHash(
    vault: AztecAddress,
    payload: ExecutionPayload,
    txNonce: Fr,
    expected: string,
  ): Promise<Fr> {
    let captured: Fr | undefined;
    const capture = {
      createAuthWit: (messageHash: Fr) => {
        captured = messageHash;
        return Promise.resolve(new AuthWitness(messageHash, []));
      },
    };
    const feeOptions = await this.completeFeeOptions({
      from: vault,
      feePayer: payload.feePayer,
      forEstimation: true,
    });
    await new DefaultAccountEntrypoint(vault, capture).createTxExecutionRequest(
      payload,
      feeOptions.gasSettings,
      await this.getChainInfo(),
      this.#entrypointOptions(txNonce, feeOptions),
    );
    if (!captured) throw new Error("Entrypoint did not request a witness");
    if (!captured.equals(Fr.fromString(expected)))
      throw new VaultTxError(
        "outer-hash",
        "Aztec's entrypoint hashes the payload differently from the proposal",
      );
    return captured;
  }

  async #registerStubClass(): Promise<Fr> {
    if (!this.#stubClassId) {
      await this.pxe.registerContractClass(StubSchnorrAccountContractArtifact);
      this.#stubClassId = (
        await getContractClassFromArtifact(StubSchnorrAccountContractArtifact)
      ).id;
    }
    return this.#stubClassId;
  }

  #resolveTaggingStrategy(request: TaggingSecretStrategyRequest): Promise<TaggingSecretStrategy> {
    const vault = this.#vaults.get(request.sender.toString());
    const selfSend = request.recipient.equals(request.sender);
    if (vault && selfSend && request.deliveryMode === AppTaggingSecretKind.UNCONSTRAINED) {
      // Unconstrained self-sends use the current epoch's secret, so members removed at a rotation stop finding them.
      return Promise.resolve({ type: "arbitrary-secret", secret: vault.epochTaggingSecret });
    }
    return this.#taggingStrategy?.(request) ?? Promise.resolve(DEFAULT_TAGGING_SECRET_STRATEGY);
  }

  #vault(vault: AztecAddress): VaultRecord {
    const record = this.#vaults.get(vault.toString());
    if (!record) throw new Error(`Vault ${vault.toString()} is not registered in this wallet`);
    return record;
  }
}
