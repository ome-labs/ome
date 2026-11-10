import { Fr } from "@aztec-labs/foundation/curves/bn254";
import type {
  ApproveParams,
  ExecuteParams,
  ExecutionProgress,
  Hex32,
  Intent,
  ProposalPreview,
  ProposalRef,
  ProposalView,
  ProposeParams,
  RejectParams,
  Review,
  ReviewError,
  Secret,
} from "@ome-labs/api";
import { type VaultTx, payloadContracts } from "@ome-labs/wallet";
import {
  checkProposal,
  decodeProposal,
  policyWarnings,
  rejectionMessage,
  signerSwapSlot,
} from "@ome-labs/wasm";

import type { Context } from "../context.ts";
import { decodeCalls } from "../contracts/decode.ts";
import { DaemonError, notReady, require } from "../errors.ts";
import { feeEstimate, feesStatus, requireFees } from "../fees.ts";
import { LIMITS } from "../limits.ts";
import { requireBalance } from "../vaults/tokens.ts";
import { effectsOf } from "./effects.ts";
import { reviewError, simulationError } from "./failures.ts";
import { authwitsOf, intentOf, payloadOf, summaryOf } from "./intents.ts";
import { type Stored, decodeAuthwits, proposalView, storedProposals } from "./views.ts";

/** `rotate_membership(...)`'s selector; its proposals are approved through the membership service. */
const ROTATE_MEMBERSHIP_SELECTOR = 0xe1071b7c;

/** Approves rotation proposals after comparing them with this member's own reshare (the membership service). */
export type RotationApprover = (
  vault: Hex32,
  stored: Stored,
  tx: VaultTx,
  secret: Secret | undefined,
) => Promise<string | null>;

/**
 * Proposals end to end: an intent becomes a proposal stored with the member's keeper (which replicates it), members
 * review it rebuilt on their own device and vote with their signers, and any member executes it once `k` approved.
 */
export class ProposalService {
  readonly #context: Context;
  #approveRotation: RotationApprover | undefined;

  constructor(context: Context) {
    this.#context = context;
  }

  set rotationApprover(approver: RotationApprover) {
    this.#approveRotation = approver;
  }

  #tokens(vault: Hex32) {
    return this.#context.state.file.tokens[vault] ?? [];
  }

  async list(vault: Hex32): Promise<ProposalView[]> {
    this.#context.vaults.record(vault);
    const all = await storedProposals(this.#context, vault);
    const views = await Promise.all(
      all.map((s) => proposalView(this.#context, s, all, this.#tokens(vault))),
    );
    return views.toSorted((a, b) => b.expiresAt - a.expiresAt);
  }

  async #stored(ref: ProposalRef): Promise<{ stored: Stored; all: Stored[] }> {
    this.#context.vaults.record(ref.vault);
    const all = await storedProposals(this.#context, ref.vault);
    const stored = all.find((s) => s.info.id === ref.id.toLowerCase());
    if (!stored) throw new DaemonError("not-found", `unknown proposal ${ref.id}`);
    return { stored, all };
  }

  async get(ref: ProposalRef): Promise<ProposalView> {
    const { stored, all } = await this.#stored(ref);
    return proposalView(this.#context, stored, all, this.#tokens(ref.vault));
  }

  /** Builds the transaction; `share` also records and offers the contracts its calls need, for a real proposal. */
  async #prepare(params: ProposeParams, txNonce?: Fr, share = false): Promise<VaultTx> {
    const { minExpiryHours: min, maxExpiryHours: max } = LIMITS;
    require(params.expiresInHours >= min &&
      params.expiresInHours <= max, `a proposal lasts ${min} to ${max} hours`);
    await this.#context.vaults.ensureCurrent(params.vault);
    const wallet = this.#context.wallet();
    const vault = this.#context.vaults.address(params.vault);
    const payload = await payloadOf(this.#context, params.intent, vault);
    const authwits = await authwitsOf(this.#context, params.intent);
    const contracts = share
      ? await this.#context.contracts.forProposal([
          ...payload.calls.map((call) => call.to),
          ...authwits.map((a) => a.call.to),
        ])
      : undefined;
    return wallet.prepareVaultTx(vault, payload, {
      expiresAt: (await wallet.chainTime()) + params.expiresInHours * 3600,
      summary: summaryOf(params.intent),
      proposer: this.#context.vaults.myIndex(params.vault),
      txNonce,
      authwits,
      ...(contracts ? { contracts } : {}),
    });
  }

  /** What the member knows names for in `vault`. */
  async #labels(vault: Hex32) {
    return {
      vault,
      vaultName: this.#context.vaults.record(vault).name,
      tokens: this.#tokens(vault),
      names: await this.#context.names(),
      members: this.#context.vaults.memberAccounts(vault),
    };
  }

  /**
   * Makes the contracts a proposal calls usable on this device: missing classes are fetched from the members who have
   * them and checked. Returns whether everything is here; never throws (a review still says what it can).
   */
  async ensureContracts(ref: ProposalRef): Promise<boolean> {
    try {
      const { stored } = await this.#stored(ref);
      if (!stored.payload) return false;
      const wallet = this.#context.wallet();
      const request = checkProposal(
        {
          manifest: this.#context.vaults.manifest(ref.vault),
          config: await wallet.getVaultConfig(this.#context.vaults.address(ref.vault)),
        },
        await wallet.chain(),
        stored.body,
        stored.payload,
        Math.min(await wallet.chainTime(), stored.info.expiresAt),
      );
      return await this.#context.contracts.ensure(
        ref.vault,
        [...request.calls, ...request.authwits.map((a) => a.call)].map((call) => call.to),
        payloadContracts(request),
      );
    } catch {
      return false;
    }
  }

  /**
   * A dry run. An intent that can't become calls (a bad argument, an unknown contract) fails the request; a
   * transaction that doesn't simulate is reported with its failure.
   */
  async preview(params: ProposeParams): Promise<ProposalPreview> {
    const tx = await this.#prepare(params);
    const labels = await this.#labels(params.vault);
    const calls = await decodeCalls(this.#context, tx.payload.calls, labels);
    const authwits = await decodeAuthwits(this.#context, tx, calls, labels);
    return { calls, authwits, ...(await this.#simulate(params.vault, tx)) };
  }

  /** Simulates `tx` and reads its effects on the vault; a failure becomes a kind with its message. */
  async #simulate(vault: Hex32, tx: VaultTx): Promise<Review["simulation"]> {
    const wallet = this.#context.wallet();
    const tokens = this.#tokens(vault);
    try {
      const result = await wallet.previewVaultTx(tx);
      const gas = result.gasUsed.totalGas;
      const facts = await wallet.simulationFacts(tx, result);
      const effects = await effectsOf(this.#context, vault, tx.payload.calls, facts, tokens);
      const used = { da: gas.daGas, l2: gas.l2Gas };
      const fee = await feeEstimate(this.#context, used).catch(() => null);
      return { ok: true, gas: used, error: null, effects, fee };
    } catch (error) {
      return {
        ok: false,
        gas: null,
        error: simulationError(error, tokens),
        effects: null,
        fee: null,
      };
    }
  }

  /** Builds a proposal and stores it with the keeper, which replicates it to every member. */
  async create(params: ProposeParams, txNonce?: Fr): Promise<ProposalView> {
    if (params.intent.kind === "transfer")
      await requireBalance(this.#context, params.vault, params.intent.token, params.intent.amount);
    const tx = await this.#prepare(params, txNonce, true);
    const keeper = this.#context.keeper.client;
    await keeper.call("payloads.put", { vault: params.vault, body: tx.payloadBody });
    await keeper.call("proposals.put", { vault: params.vault, body: tx.proposal });
    return this.get({ vault: params.vault, id: tx.id });
  }

  /** Proposes cancelling `ref`: a fee-only proposal under its nonce, which burns the nonce when it runs. */
  async cancel(ref: ProposalRef): Promise<ProposalView> {
    const { stored } = await this.#stored(ref);
    const intent: Intent = { kind: "cancel", cancels: stored.info.id, memo: null };
    return this.create(
      { vault: ref.vault, intent, expiresInHours: 24 },
      Fr.fromString(stored.info.txNonce),
    );
  }

  /** The transaction a stored proposal describes, rebuilt and checked on this device. */
  async #rebuilt(stored: Stored): Promise<VaultTx> {
    if (!stored.payload) throw notReady("payload", "the proposal's payload has not arrived yet");
    await this.#context.vaults.ensureCurrent(stored.vault);
    return this.#context
      .wallet()
      .vaultTxOf(this.#context.vaults.address(stored.vault), stored.body, stored.payload);
  }

  async review(ref: ProposalRef): Promise<Review> {
    await this.ensureContracts(ref);
    const { stored, all } = await this.#stored(ref);
    const proposal = await proposalView(this.#context, stored, all, this.#tokens(ref.vault));
    const record = this.#context.vaults.record(ref.vault);
    let failure: ReviewError | null = null;
    let tx: VaultTx | undefined;
    try {
      tx = await this.#rebuilt(stored);
    } catch (error) {
      failure = reviewError(error);
    }
    const simulation: Review["simulation"] = tx
      ? await this.#simulate(ref.vault, tx)
      : {
          ok: false,
          gas: null,
          error: {
            failure: { kind: "unavailable" },
            message: "the proposal didn't rebuild on this device",
          },
          effects: null,
          fee: null,
        };
    const intent = proposal.intent;
    // A member's own account is named as the member, ahead of an aztec-wallet alias nobody verified.
    const member =
      intent.kind === "transfer"
        ? (this.#context.vaults.memberAccounts(ref.vault).get(intent.to) ?? null)
        : null;
    const alias =
      intent.kind === "transfer" ? ((await this.#context.names()).get(intent.to) ?? null) : null;
    const label = member ?? alias;
    return {
      proposal,
      vaultName: record.name,
      myName: this.#context.state.file.profile?.name ?? "",
      checks: {
        rebuilt: tx !== undefined,
        outerHash: stored.info.outerHash,
        configCurrent: proposal.configVersion === proposal.currentConfigVersion,
        notExpired: proposal.status !== "expired",
        failure,
      },
      action: actionOf(proposal),
      change: await this.#change(ref.vault, intent),
      recipient:
        intent.kind === "transfer"
          ? {
              address: intent.to,
              label,
              source: member ? "member" : alias ? "aztecWallet" : null,
            }
          : null,
      simulation,
      signs: {
        message: proposal.approvalMessage,
        formula: 'Poseidon2("ome.approval.v0", [vault, outerHash, configVersion, expiresAt])',
      },
      rejects: {
        message: rejectionMessage(ref.vault, stored.info.id),
        formula: 'Poseidon2("ome.rejection.v0", [vault, id_hi, id_lo])',
      },
    };
  }

  /** For governance intents: the vault's policy before and after, with the new shape's risks. */
  async #change(vault: Hex32, intent: Intent): Promise<Review["change"]> {
    if (intent.kind !== "setThreshold" && intent.kind !== "invalidatePending") return null;
    const config = await this.#context.wallet().getVaultConfig(this.#context.vaults.address(vault));
    if (intent.kind === "invalidatePending")
      return { kind: "invalidatePending", version: config.version };
    return {
      kind: "threshold",
      before: config.threshold,
      after: intent.threshold,
      members: config.signerCount,
      warnings: policyWarnings(intent.threshold, config.viewingThreshold, config.signerCount),
    };
  }

  /**
   * Refuses to sign when this profile's key is not this member's key in the vault: after a signer swap the member
   * votes on the paired device that holds it.
   */
  #requireLocalKey(vault: Hex32): void {
    const key = this.#context.state.file.profile?.signerKey;
    const manifest = this.#context.vaults.manifest(vault);
    const mine = manifest.members[this.#context.vaults.myIndex(vault) - 1]?.signerKey;
    if (key && mine && (mine.x !== key.x || mine.y !== key.y))
      throw new DaemonError(
        "invalid",
        "your signing key in this vault is on a paired device: approve or reject there",
      );
  }

  /** Approves after rebuilding and checking; a rotation is compared with this member's own reshare first. */
  async approve(params: ApproveParams): Promise<ProposalView> {
    const { stored } = await this.#stored(params);
    this.#requireLocalKey(params.vault);
    const tx = await this.#rebuilt(stored);
    const rotation = tx.payload.calls.some(
      (c) => c.to.toString() === params.vault && c.selector.value === ROTATE_MEMBERSHIP_SELECTOR,
    );
    const wallet = this.#context.wallet();
    const input = await wallet.vaultInput(tx.vault);
    let approval: string | null;
    if (rotation && signerSwapSlot(input, stored.payload ?? "") !== null) {
      const chain = await wallet.chain();
      const now = await wallet.chainTime();
      const feePayers = [this.#context.feePayer().toString()];
      approval = this.#context.signer.with(params.secret, (signer) =>
        signer.approveSignerSwap(input, chain, stored.body, stored.payload ?? "", feePayers, now),
      );
    } else if (rotation) {
      if (!this.#approveRotation) throw notReady("starting", "rotations are not available");
      approval = await this.#approveRotation(params.vault, stored, tx, params.secret);
    } else {
      const chain = await wallet.chain();
      const now = await wallet.chainTime();
      approval = this.#context.signer.with(params.secret, (signer) =>
        signer.approve(input, chain, stored.body, stored.payload ?? "", now),
      );
    }
    if (approval)
      await this.#context.keeper.client.call("approvals.put", {
        vault: params.vault,
        body: approval,
      });
    return this.get(params);
  }

  async reject(params: RejectParams): Promise<ProposalView> {
    const { stored } = await this.#stored(params);
    this.#requireLocalKey(params.vault);
    await this.#context.vaults.ensureCurrent(params.vault);
    const input = await this.#context
      .wallet()
      .vaultInput(this.#context.vaults.address(params.vault));
    const rejection = this.#context.signer.with(params.secret, (signer) =>
      signer.reject(input, stored.body, params.reason ?? ""),
    );
    await this.#context.keeper.client.call("rejections.put", {
      vault: params.vault,
      body: rejection,
    });
    return this.get(params);
  }

  /** Executes a ready proposal from this device and stores the receipt with the keeper. */
  /**
   * Executes a ready proposal. Another member's live claim refuses it with `conflict` unless `force`: both would
   * prove, and only one transaction can land. This member's own claim follows each phase, so every member sees how
   * far it is.
   */
  async execute(params: ExecuteParams): Promise<ProposalView> {
    const ref = { vault: params.vault, id: params.id };
    await this.ensureContracts(ref);
    const { stored } = await this.#stored(ref);
    const executing = (await this.get(ref)).executing;
    if (executing && !executing.mine && !params.force)
      throw DaemonError.detailed(
        {
          kind: "conflict",
          member: executing.member,
          phase: executing.phase,
          since: executing.since,
        },
        `${executing.member.name} is executing this proposal (${executing.phase}); execute with force to prove it too`,
      );
    const intent = intentOf(stored.info.summary);
    if (intent.kind === "transfer")
      await requireBalance(this.#context, ref.vault, intent.token, intent.amount);
    const keeper = this.#context.keeper.client;
    const startedAt = Math.floor(Date.now() / 1000);
    const progress = async (
      phase: ExecutionProgress["phase"],
      extra: Partial<ExecutionProgress> = {},
    ) => {
      this.#context.emit({
        type: "execution",
        execution: {
          vault: ref.vault,
          proposal: stored.info.id,
          phase,
          error: null,
          txHash: null,
          ...extra,
        },
      });
      // The claim is a courtesy to the other members; an unreachable keeper never stops the execution.
      await keeper
        .call("claims.put", { vault: ref.vault, proposal: stored.info.id, phase, startedAt })
        .catch(() => undefined);
    };
    await progress("rebuilding");
    try {
      const tx = await this.#rebuilt(stored);
      await this.#requirePayable(ref.vault, tx);
      await progress("proving");
      const receipt = await this.#context.wallet().executeVaultTx(tx, stored.approvals);
      const txHash = receipt.txHash.toString();
      await keeper.call("receipts.put", {
        vault: ref.vault,
        proposal: tx.id,
        txHash,
        block: receipt.blockNumber ?? 0,
      });
      await progress("mined", { txHash });
      // A governance call changed the config: adopt it now, not at the next sync.
      await this.#context.vaults.observe(ref.vault).catch(() => undefined);
    } catch (error) {
      await progress("failed", {
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
    return this.get(ref);
  }

  /**
   * Refuses before proving when the fee payer can't cover `tx`. A healthy sponsored payer is trusted without a
   * simulation; the member's own payer, or one with no Fee Juice, is checked against `tx`'s simulated cost.
   */
  async #requirePayable(vault: Hex32, tx: VaultTx): Promise<void> {
    const status = await feesStatus(this.#context);
    if (status.kind === "sponsored" && status.healthy) return;
    const simulation = await this.#simulate(vault, tx);
    if (simulation.fee) await requireFees(this.#context, simulation.fee);
  }

  /** The proposal id of a stored body. */
  static idOf(body: string): string {
    return decodeProposal(body).id;
  }
}

/** One line saying what a proposal does, with exact numbers. */
function actionOf(proposal: ProposalView): string {
  const { intent } = proposal;
  switch (intent.kind) {
    case "transfer":
      return `transfer ${intent.amount} base units of ${proposal.token?.symbol ?? intent.token}`;
    case "cancel":
      return `cancel proposal ${intent.cancels}`;
    case "setThreshold":
      return `need ${intent.threshold} approvals to spend`;
    case "invalidatePending":
      return "void every approval collected so far";
    case "calls":
      return proposal.authwits.length > 0
        ? `${proposal.calls.length} call(s), granting ${proposal.authwits.length} authwit(s)`
        : `${proposal.calls.length} call(s)`;
  }
}
