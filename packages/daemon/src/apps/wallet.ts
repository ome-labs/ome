// The vault as an Aztec `Wallet`, for one app the member allowed. Reads and simulations are answered from the vault's
// PXE; a transaction becomes a proposal, and the call returns once the members approved and it executed. The app sees
// the vault as its only account, never the member's own account or address book.
import {
  type CallIntent,
  type IntentInnerHash,
  computeAuthWitMessageHash,
} from "@aztec-labs/aztec.js/authorization";
import { NO_WAIT } from "@aztec-labs/aztec.js/contracts";
import {
  type Aliased,
  type AppCapabilities,
  type ExecuteUtilityOptions,
  type GrantedCapability,
  type PrivateEventFilter,
  type SendOptions,
  type SimulateOptions,
  TxSimulationResultWithAppOffset,
  type WalletCapabilities,
} from "@aztec-labs/aztec.js/wallet";
import type { Fr } from "@aztec-labs/foundation/curves/bn254";
import type {
  ContractArtifact,
  EventMetadataDefinition,
  FunctionCall,
} from "@aztec-labs/stdlib/abi";
import { AuthWitness } from "@aztec-labs/stdlib/auth-witness";
import type { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { ContractInstancePreimage } from "@aztec-labs/stdlib/contract";
import { ExecutionPayload, TxHash } from "@aztec-labs/stdlib/tx";
import type { Hex32, ProposalView } from "@ome-labs/api";
import type { VaultAuthwit } from "@ome-labs/wallet";

import { type Context, until } from "../context.ts";
import { DaemonError } from "../errors.ts";
import type { AppRecord } from "../profile/state.ts";
import type { ProposalService } from "../proposals/service.ts";

/** How long an app's `sendTx` waits for the members before it returns an error naming the proposal. */
export const APP_WAIT_MS = 15 * 60_000;
/** How long an app's proposal stays open. */
const APP_EXPIRY_HOURS = 24;

const DONE = new Set<ProposalView["status"]>([
  "executed",
  "cancelled",
  "expired",
  "stale",
  "blocked",
]);

export class AppWallet {
  readonly #context: Context;
  readonly #proposals: ProposalService;
  readonly #origin: string;
  readonly #app: AppRecord;
  /** Authwits the app asked for since its last transaction, granted with the next one. */
  readonly #pending: VaultAuthwit[];
  readonly #waitMs: number;

  constructor(
    context: Context,
    proposals: ProposalService,
    origin: string,
    app: AppRecord,
    pending: VaultAuthwit[],
    waitMs = APP_WAIT_MS,
  ) {
    this.#context = context;
    this.#proposals = proposals;
    this.#origin = origin;
    this.#app = app;
    this.#pending = pending;
    this.#waitMs = waitMs;
  }

  get #vault(): AztecAddress {
    return this.#context.vaults.address(this.#app.vault);
  }

  #wallet() {
    return this.#context.wallet();
  }

  /** Refuses any account but the vault this app may use. */
  #requireVault(from: AztecAddress | string, what: string): void {
    if (typeof from === "string" || !from.equals(this.#vault))
      throw new DaemonError("invalid", `${what} must be from the vault this app may use`);
  }

  /** Only the vault's notes and keys are visible to the app. */
  #requireScopes(scopes: readonly AztecAddress[]): AztecAddress[] {
    if (!scopes.every((scope) => scope.equals(this.#vault)))
      throw new DaemonError("invalid", "an app reads only the vault it may use");
    return [this.#vault];
  }

  getChainInfo() {
    return this.#wallet().getChainInfo();
  }

  getContractMetadata(address: AztecAddress) {
    return this.#wallet().getContractMetadata(address);
  }

  getContractClassMetadata(id: Fr) {
    return this.#wallet().getContractClassMetadata(id);
  }

  registerSender(address: AztecAddress, alias?: string) {
    return this.#wallet().registerSender(address, alias);
  }

  getAddressBook(): Promise<Aliased<AztecAddress>[]> {
    return Promise.resolve([]);
  }

  async getAccounts(): Promise<Aliased<AztecAddress>[]> {
    const name = this.#context.vaults.record(this.#app.vault).name;
    return [{ alias: name, item: this.#vault }];
  }

  registerContract(instance: ContractInstancePreimage, artifact?: ContractArtifact) {
    return this.#wallet().registerContract(instance, artifact);
  }

  registerContractClass(artifact: ContractArtifact) {
    return this.#wallet().registerContractClass(artifact);
  }

  executeUtility(call: FunctionCall, opts: ExecuteUtilityOptions) {
    return this.#wallet().executeUtility(call, {
      ...opts,
      scopes: this.#requireScopes(opts.scopes),
    });
  }

  getPrivateEvents<T>(metadata: EventMetadataDefinition, filter: PrivateEventFilter) {
    return this.#wallet().getPrivateEvents<T>(metadata, {
      ...filter,
      scopes: this.#requireScopes(filter.scopes),
    });
  }

  /**
   * Simulates the app's transaction as the vault would run it: with the member's fee payer and the authwits the app
   * asked for, against the vault's preview (no approvals needed). The app's calls start after the entrypoint and the
   * fee call.
   */
  async simulateTx(exec: ExecutionPayload, opts: SimulateOptions) {
    this.#requireVault(opts.from, "a simulation");
    const tx = await this.#proposals.appTx(this.#params(), this.#own(exec), [...this.#pending]);
    const result = await this.#wallet().previewVaultTx(tx);
    const fee = await this.#context.fee().paymentMethod.getExecutionPayload();
    return TxSimulationResultWithAppOffset.fromResultAndOffset(result, fee.calls.length + 1);
  }

  profileTx(): Promise<never> {
    return Promise.reject(
      new DaemonError("invalid", "Ome doesn't profile vault transactions for apps"),
    );
  }

  /**
   * Proposes the app's transaction, with the authwits it asked for, and waits until it executes: the members approve
   * it in Ome, and this daemon executes it once it is ready. Returns as `sendTx` does; past the wait, or when the
   * members block or cancel it, fails naming the proposal.
   */
  async sendTx(exec: ExecutionPayload, opts: SendOptions) {
    this.#requireVault(opts.from, "a transaction");
    const authwits = this.#pending.splice(0);
    const view = await this.#proposals.createFromApp(this.#params(), this.#own(exec), authwits);
    this.#context.emit({
      type: "appRequest",
      origin: this.#origin,
      vault: this.#app.vault,
      proposal: view.id,
    });
    const ref = { vault: this.#app.vault, id: view.id };
    const done = await until(
      async () => {
        const current = await this.#proposals.get(ref);
        // The app is waiting on this daemon, so it executes the proposal as soon as it is ready.
        if (current.status === "ready" && !current.executing) {
          await this.#proposals.execute(ref);
          return undefined;
        }
        return DONE.has(current.status) ? current : undefined;
      },
      `proposal ${view.id} from ${this.#origin}`,
      this.#waitMs,
      2000,
    ).catch((error: unknown) => {
      if (error instanceof Error && error.message.startsWith("timed out"))
        throw new DaemonError(
          "not-ready",
          `proposal ${view.id} waits for the vault's members; it runs once they approve it in Ome`,
        );
      throw error;
    });
    if (done.status !== "executed" || !done.execution)
      throw new DaemonError(
        "invalid",
        `the members didn't run proposal ${view.id}: it is ${done.status}`,
      );
    const txHash = TxHash.fromString(done.execution.txHash);
    const offchain = { offchainEffects: [], offchainMessages: [] };
    if (opts.wait === NO_WAIT) return { txHash, ...offchain };
    const receipt = await this.#context.node.node.getTxReceipt(txHash);
    return { receipt, ...offchain };
  }

  /**
   * Records an authwit the app needs for its next transaction. The vault can't sign alone: the members approve the
   * authwit with that transaction, so the app gets a placeholder witness, which the proposal's real one replaces.
   * Only an authwit the members can read (a caller and a call) is accepted, never a bare hash.
   */
  async createAuthWit(
    from: AztecAddress,
    intent: IntentInnerHash | CallIntent,
  ): Promise<AuthWitness> {
    this.#requireVault(from, "an authwit");
    if (!("caller" in intent) || !("call" in intent))
      throw new DaemonError(
        "invalid",
        "Ome grants an authwit only for a call it can show its members, not a bare hash",
      );
    const authwit = { caller: intent.caller, call: intent.call };
    this.#pending.push(authwit);
    const hash = await computeAuthWitMessageHash(authwit, await this.getChainInfo());
    return new AuthWitness(hash, []);
  }

  /** Grants what the app asks for, with the vault as its only account. */
  async requestCapabilities(manifest: AppCapabilities): Promise<WalletCapabilities> {
    const accounts = await this.getAccounts();
    const granted = manifest.capabilities.map(
      (capability) =>
        (capability.type === "accounts"
          ? { ...capability, accounts }
          : capability) as GrantedCapability,
    );
    return { version: manifest.version, granted, wallet: { name: "Ome", version: "0.1.0" } };
  }

  async batch(methods: readonly { name: string; args: unknown[] }[]) {
    const results = [];
    for (const { name, args } of methods) {
      const method = (this as unknown as Record<string, (...a: unknown[]) => Promise<unknown>>)[
        name
      ];
      if (typeof method !== "function")
        throw new DaemonError("invalid", `unknown wallet method ${name}`);
      // oxlint-disable-next-line no-await-in-loop
      results.push({ name, result: await method.apply(this, args) });
    }
    return results;
  }

  #params() {
    const intent = {
      kind: "app" as const,
      origin: this.#origin,
      name: this.#app.name,
      memo: null,
    };
    return { vault: this.#app.vault as Hex32, intent, expiresInHours: APP_EXPIRY_HOURS };
  }

  /**
   * The app's payload without the placeholder witnesses {@link createAuthWit} returned (the only empty ones): the
   * proposal carries those authwits, and execution adds their real witnesses.
   */
  #own(exec: ExecutionPayload): ExecutionPayload {
    return new ExecutionPayload(
      exec.calls,
      exec.authWitnesses.filter((w) => w.witness.length > 0),
      exec.capsules,
      exec.extraHashedArgs,
      exec.feePayer,
    );
  }
}
