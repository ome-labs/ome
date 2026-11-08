import { Fr } from "@aztec-labs/foundation/curves/bn254";
import type {
  ApproveParams,
  ExecutionProgress,
  Hex32,
  Intent,
  ProposalPreview,
  ProposalRef,
  ProposalView,
  ProposeParams,
  RejectParams,
  Review,
} from "@ome-labs/api";
import type { VaultTx } from "@ome-labs/wallet";
import { decodeProposal, rejectionMessage } from "@ome-labs/wasm";

import type { Context } from "../context.ts";
import { DaemonError, require } from "../errors.ts";
import { payloadOf, summaryOf } from "./intents.ts";
import { type Stored, proposalView, storedProposals } from "./views.ts";

/** `rotate_membership(...)`'s selector; its proposals are approved through the membership service. */
const ROTATE_MEMBERSHIP_SELECTOR = 0x4eec0a41;
const MAX_HOURS = 720;

/** Approves rotation proposals after comparing them with this member's own reshare (the membership service). */
export type RotationApprover = (
  vault: Hex32,
  stored: Stored,
  tx: VaultTx,
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

  async #prepare(params: ProposeParams, txNonce?: Fr): Promise<VaultTx> {
    require(params.expiresInHours >= 1 &&
      params.expiresInHours <= MAX_HOURS, "a proposal lasts 1 to 720 hours");
    const wallet = this.#context.wallet();
    const vault = this.#context.vaults.address(params.vault);
    const payload = await payloadOf(this.#context, params.intent);
    return wallet.prepareVaultTx(vault, payload, {
      expiresAt: (await wallet.chainTime()) + params.expiresInHours * 3600,
      summary: summaryOf(params.intent),
      proposer: this.#context.vaults.myIndex(params.vault),
      txNonce,
    });
  }

  async preview(params: ProposeParams): Promise<ProposalPreview> {
    try {
      const tx = await this.#prepare(params);
      const calls = tx.payload.calls.map((c) => ({
        to: c.to.toString(),
        toLabel: null,
        fn: c.name,
        args: c.args.map((a) => a.toString()),
        isStatic: c.isStatic,
      }));
      const simulation = await this.#context.wallet().previewVaultTx(tx);
      const gas = simulation.gasUsed.totalGas;
      return { ok: true, calls, gas: { da: gas.daGas, l2: gas.l2Gas }, error: null };
    } catch (error) {
      return {
        ok: false,
        calls: [],
        gas: null,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /** Builds a proposal and stores it with the keeper, which replicates it to every member. */
  async create(params: ProposeParams, txNonce?: Fr): Promise<ProposalView> {
    const tx = await this.#prepare(params, txNonce);
    const keeper = this.#context.keeper.client;
    await keeper.call("payloads.put", { vault: params.vault, body: tx.payloadBody });
    await keeper.call("proposals.put", { vault: params.vault, body: tx.proposal });
    this.#context.emit({ type: "vaultActivity", vault: params.vault });
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
    if (!stored.payload)
      throw new DaemonError("not-ready", "the proposal's payload has not arrived yet");
    return this.#context
      .wallet()
      .vaultTxOf(this.#context.vaults.address(stored.vault), stored.body, stored.payload);
  }

  async review(ref: ProposalRef): Promise<Review> {
    const { stored, all } = await this.#stored(ref);
    const proposal = await proposalView(this.#context, stored, all, this.#tokens(ref.vault));
    const record = this.#context.vaults.record(ref.vault);
    let failure: string | null = null;
    let tx: VaultTx | undefined;
    try {
      tx = await this.#rebuilt(stored);
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    }
    let simulation: Review["simulation"] = { ok: false, gas: null, error: failure };
    if (tx) {
      try {
        const gas = (await this.#context.wallet().previewVaultTx(tx)).gasUsed.totalGas;
        simulation = { ok: true, gas: { da: gas.daGas, l2: gas.l2Gas }, error: null };
      } catch (error) {
        simulation = {
          ok: false,
          gas: null,
          error: error instanceof Error ? error.message : String(error),
        };
      }
    }
    const intent = proposal.intent;
    const contact =
      intent.kind === "transfer"
        ? this.#context.state.file.contacts.find((c) => c.address === intent.to)
        : undefined;
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
      action:
        intent.kind === "transfer"
          ? `transfer ${intent.amount} base units of ${proposal.token?.symbol ?? intent.token}`
          : intent.kind === "cancel"
            ? `cancel proposal ${intent.cancels}`
            : `${proposal.calls.length} call(s)`,
      recipient:
        intent.kind === "transfer"
          ? {
              address: intent.to,
              label: contact?.label ?? null,
              source: contact ? "addressBook" : null,
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

  /** Approves after rebuilding and checking; a rotation is compared with this member's own reshare first. */
  async approve(params: ApproveParams): Promise<ProposalView> {
    const { stored } = await this.#stored(params);
    const tx = await this.#rebuilt(stored);
    const rotation = tx.payload.calls.some(
      (c) => c.to.toString() === params.vault && c.selector.value === ROTATE_MEMBERSHIP_SELECTOR,
    );
    let approval: string | null;
    if (rotation) {
      if (!this.#approveRotation) throw new DaemonError("not-ready", "rotations are not available");
      approval = await this.#approveRotation(params.vault, stored, tx);
    } else {
      const wallet = this.#context.wallet();
      const input = await wallet.vaultInput(tx.vault);
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
    this.#context.emit({ type: "vaultActivity", vault: params.vault });
    return this.get(params);
  }

  async reject(params: RejectParams): Promise<ProposalView> {
    const { stored } = await this.#stored(params);
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
    this.#context.emit({ type: "vaultActivity", vault: params.vault });
    return this.get(params);
  }

  /** Executes a ready proposal from this device and stores the receipt with the keeper. */
  async execute(ref: ProposalRef): Promise<ProposalView> {
    const { stored } = await this.#stored(ref);
    const progress = (phase: ExecutionProgress["phase"], extra: Partial<ExecutionProgress> = {}) =>
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
    progress("rebuilding");
    try {
      const tx = await this.#rebuilt(stored);
      progress("proving");
      const receipt = await this.#context.wallet().executeVaultTx(tx, stored.approvals);
      const txHash = receipt.txHash.toString();
      await this.#context.keeper.client.call("receipts.put", {
        vault: ref.vault,
        proposal: tx.id,
        txHash,
      });
      progress("mined", { txHash });
    } catch (error) {
      progress("failed", { error: error instanceof Error ? error.message : String(error) });
      throw error;
    }
    return this.get(ref);
  }

  /** The proposal id of a stored body. */
  static idOf(body: string): string {
    return decodeProposal(body).id;
  }
}
