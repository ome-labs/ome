// The proposal events clients follow, derived from the keeper's: it reports every proposal, vote, execution claim and
// receipt it stores, whether this member made it or a member's keeper replicated it, so one place covers both.
// Expiry has no keeper event: a periodic `tick` announces proposals about to expire and those that did.
import type {
  DaemonEvent,
  ExecutionPhase,
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
type List = (vault: Hex32) => Promise<ProposalView[]>;
type Member = (vault: Hex32, index: number) => MemberRef;
type Now = () => Promise<number>;

/** How long before its deadline an open proposal is announced as expiring. */
export const EXPIRING_SECS = 3600;

const OPEN: ReadonlySet<ProposalStatus> = new Set(["collecting", "ready", "blocked"]);

/** What the events need besides reading one proposal: its vault's proposals, member names and chain time. */
export interface EventSources {
  readonly list: List;
  readonly member: Member;
  readonly now: Now;
}

const key = (ref: ProposalRef) => `${ref.vault}:${ref.id}`;

export class ProposalEvents {
  readonly #view: View;
  readonly #emit: Emit;
  readonly #prepare: Prepare;
  readonly #sources: EventSources | undefined;
  /** Proposals already announced as expiring, and as expired: once each per daemon run. */
  readonly #expiring = new Set<string>();
  readonly #expired = new Set<string>();
  /** The status last announced per proposal, so ready and blocked are announced once each. */
  readonly #status = new Map<string, ProposalStatus>();
  /** Proposals whose payload hadn't arrived when they did: announced once it does. */
  readonly #waiting = new Map<Hex32, Set<Hex32>>();

  /** `prepare` starts fetching the contracts a proposal calls, as soon as it is here. */
  constructor(
    view: View,
    emit: Emit,
    prepare: Prepare = () => Promise.resolve(),
    sources?: EventSources,
  ) {
    this.#view = view;
    this.#emit = emit;
    this.#prepare = prepare;
    this.#sources = sources;
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
      case "claim":
        return this.#executing(
          { vault: event.vault, id: event.proposal },
          event.member,
          event.phase,
        );
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
    const vote =
      view.votes.find((v) => v.slot === slot && v.approve) ??
      view.votes.find((v) => v.slot === slot);
    if (!vote) return;
    const by: MemberRef = { index: vote.index, name: vote.name };
    this.#emit({
      type: "proposalVoted",
      vault: ref.vault,
      proposal: ref.id,
      summary: summaryLine(view),
      by,
      approve: vote.approve,
      reason: vote.reason,
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
      by: view.execution?.executor ?? null,
    });
    if (view.intent.kind === "transfer")
      this.#emit({ type: "balanceChanged", vault: ref.vault, token: view.intent.token });
    if (view.intent.kind === "cancel") {
      const target = { vault: ref.vault, id: view.intent.cancels };
      const cancelled = await this.#view(target).catch(() => undefined);
      this.#status.set(key(target), "cancelled");
      this.#emit({
        type: "proposalCancelled",
        vault: ref.vault,
        proposal: target.id,
        summary: cancelled ? summaryLine(cancelled) : `proposal ${target.id}`,
        byProposal: ref.id,
      });
    }
  }

  async #executing(ref: ProposalRef, member: number, phase: ExecutionPhase): Promise<void> {
    const view = await this.#view(ref).catch(() => undefined);
    if (!view) return;
    this.#emit({
      type: "proposalExecuting",
      vault: ref.vault,
      proposal: ref.id,
      summary: summaryLine(view),
      by: this.#sources?.member(ref.vault, member) ?? { index: member, name: `member ${member}` },
      phase,
    });
  }

  /**
   * Announces, once each, open proposals of `vaults` within `EXPIRING_SECS` of their deadline and those past it.
   * Runs with the daemon's periodic sync.
   */
  async tick(vaults: readonly Hex32[]): Promise<void> {
    if (!this.#sources) return;
    const now = await this.#sources.now();
    for (const vault of vaults) {
      // oxlint-disable-next-line no-await-in-loop
      const views = await this.#sources.list(vault).catch(() => []);
      for (const view of views) this.#expiry(view, now);
    }
  }

  #expiry(view: ProposalView, now: number): void {
    const id = key({ vault: view.vault, id: view.id });
    const summary = summaryLine(view);
    if (view.status === "expired" && !this.#expired.has(id)) {
      this.#expired.add(id);
      this.#emit({ type: "proposalExpired", vault: view.vault, proposal: view.id, summary });
      return;
    }
    if (OPEN.has(view.status) && view.expiresAt - now <= EXPIRING_SECS && !this.#expiring.has(id)) {
      this.#expiring.add(id);
      this.#emit({
        type: "proposalExpiring",
        vault: view.vault,
        proposal: view.id,
        summary,
        at: view.expiresAt,
      });
    }
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
