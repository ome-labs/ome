// The proposal events clients follow, derived from the keeper's: it reports every proposal, vote and receipt it
// stores, whether this member made it or a member's keeper replicated it, so one place covers both.
import type {
  DaemonEvent,
  Hex32,
  KeeperEvent,
  MemberRef,
  ProposalRef,
  ProposalStatus,
  ProposalView,
} from "@ome-labs/api";

import { summaryLine } from "./summary.ts";

type Emit = (event: DaemonEvent) => void;
type View = (ref: ProposalRef) => Promise<ProposalView>;
type Prepare = (ref: ProposalRef) => Promise<unknown>;

const key = (ref: ProposalRef) => `${ref.vault}:${ref.id}`;

export class ProposalEvents {
  readonly #view: View;
  readonly #emit: Emit;
  readonly #prepare: Prepare;
  /** The status last announced per proposal, so ready and blocked are announced once each. */
  readonly #status = new Map<string, ProposalStatus>();
  /** Proposals whose payload hadn't arrived when they did: announced once it does. */
  readonly #waiting = new Map<Hex32, Set<Hex32>>();

  /** `prepare` starts fetching the contracts a proposal calls, as soon as it is here. */
  constructor(view: View, emit: Emit, prepare: Prepare = () => Promise.resolve()) {
    this.#view = view;
    this.#emit = emit;
    this.#prepare = prepare;
  }

  /** Reacts to one keeper event. Failures to read a proposal are left to the next event; they never throw. */
  async onKeeperEvent(event: KeeperEvent): Promise<void> {
    switch (event.type) {
      case "proposal":
        return this.#created({ vault: event.vault, id: event.id });
      case "payload":
        return this.#arrived(event.vault);
      case "approval":
      case "rejection":
        return this.#voted({ vault: event.vault, id: event.proposal }, event.slot);
      case "receipt":
        return this.#executed({ vault: event.vault, id: event.proposal }, event.txHash);
      default:
        return undefined;
    }
  }

  async #created(ref: ProposalRef): Promise<void> {
    void this.#prepare(ref).catch(() => undefined);
    const view = await this.#view(ref).catch(() => undefined);
    if (!view) {
      const waiting = this.#waiting.get(ref.vault) ?? new Set();
      waiting.add(ref.id);
      this.#waiting.set(ref.vault, waiting);
      return;
    }
    this.#emit({
      type: "proposalCreated",
      vault: ref.vault,
      proposal: ref.id,
      summary: summaryLine(view),
      by: { index: view.proposer.index, name: view.proposer.name },
    });
    this.#transition(ref, view);
  }

  async #arrived(vault: Hex32): Promise<void> {
    const waiting = this.#waiting.get(vault);
    if (!waiting) return;
    this.#waiting.delete(vault);
    await Promise.all([...waiting].map((id) => this.#created({ vault, id })));
  }

  async #voted(ref: ProposalRef, slot: number): Promise<void> {
    const view = await this.#view(ref).catch(() => undefined);
    if (!view) return;
    const approval = view.approvals.find((vote) => vote.slot === slot);
    const rejection = view.rejections.find((vote) => vote.slot === slot);
    const vote = approval ?? rejection;
    if (!vote) return;
    const by: MemberRef = { index: vote.index, name: vote.name };
    this.#emit({
      type: "proposalVoted",
      vault: ref.vault,
      proposal: ref.id,
      summary: summaryLine(view),
      by,
      approve: approval !== undefined,
      reason: rejection?.reason ?? null,
    });
    this.#transition(ref, view);
  }

  async #executed(ref: ProposalRef, txHash: Hex32): Promise<void> {
    const view = await this.#view(ref).catch(() => undefined);
    if (!view) return;
    this.#status.set(key(ref), "executed");
    this.#emit({
      type: "proposalExecuted",
      vault: ref.vault,
      proposal: ref.id,
      summary: summaryLine(view),
      txHash,
    });
    if (view.intent.kind === "transfer")
      this.#emit({ type: "balanceChanged", vault: ref.vault, token: view.intent.token });
  }

  /** Announces a proposal becoming ready or blocked, once. */
  #transition(ref: ProposalRef, view: ProposalView): void {
    const before = this.#status.get(key(ref));
    this.#status.set(key(ref), view.status);
    if (view.status === before) return;
    if (view.status === "ready" || view.status === "blocked")
      this.#emit({
        type: view.status === "ready" ? "proposalReady" : "proposalBlocked",
        vault: ref.vault,
        proposal: ref.id,
        summary: summaryLine(view),
      });
  }
}
