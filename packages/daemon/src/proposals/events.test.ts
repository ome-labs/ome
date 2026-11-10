// Clients follow proposals through the daemon's events: one per proposal that arrives (also once its payload catches up
// with it), one per vote with who cast it, ready and blocked once each, who is executing it, executed with who did and
// the balance it touched, cancellations, and expiring and expired once each.
import type {
  DaemonEvent,
  Hex32,
  KeeperEvent,
  ProposalRef,
  ProposalView,
  Vote,
} from "@ome-labs/api";
import { describe, expect, it } from "vitest";

import { ProposalEvents } from "./events.ts";

const VAULT = `0x${"aa".repeat(32)}` as Hex32;
const ID = `0x${"bb".repeat(32)}` as Hex32;
const TOKEN = `0x${"cc".repeat(32)}` as Hex32;
const TO = `0xf941${"00".repeat(28)}12b3` as Hex32;

const vote = (slot: number, name: string, reason: string | null = null): Vote => ({
  slot,
  index: slot + 1,
  name,
  approve: reason === null,
  valid: true,
  reason,
  at: 1_900_000_000,
});

function view(patch: Partial<ProposalView> = {}): ProposalView {
  return {
    id: ID,
    vault: VAULT,
    intent: { kind: "transfer", token: TOKEN, to: TO, amount: "40000000000000000000", memo: null },
    proposer: { index: 2, name: "Bob" },
    status: "collecting",
    threshold: 2,
    members: 3,
    votes: [vote(1, "Bob")],
    expiresAt: 1_900_086_400,
    token: { address: TOKEN, name: "Ome Test", symbol: "OMET", decimals: 18 },
    ...patch,
  } as ProposalView;
}

function harness(initial: ProposalView | undefined, others: ProposalView[] = []) {
  let current = initial;
  let now = 1_900_000_000;
  const emitted: DaemonEvent[] = [];
  const events = new ProposalEvents(
    (ref: ProposalRef) => {
      const found = [current, ...others].find((v) => v && v.id === ref.id);
      return found ? Promise.resolve(found) : Promise.reject(new Error("no payload"));
    },
    (event) => emitted.push(event),
    () => Promise.resolve(),
    {
      list: () => Promise.resolve(current ? [current] : []),
      member: (_, index) => ({ index, name: ["", "Alice", "Bob", "Carol"][index] ?? "" }),
      now: () => Promise.resolve(now),
    },
  );
  const keeper = (event: KeeperEvent) => events.onKeeperEvent(event);
  const set = (next: ProposalView | undefined) => {
    current = next;
  };
  const tick = (at: number) => {
    now = at;
    return events.tick([VAULT]);
  };
  return { emitted, keeper, set, tick };
}

describe("ProposalEvents", () => {
  it("announces a proposal with its summary and proposer", async () => {
    const { emitted, keeper } = harness(view());
    await keeper({ type: "proposal", vault: VAULT, id: ID });
    expect(emitted).toEqual([
      {
        type: "proposalCreated",
        vault: VAULT,
        proposal: ID,
        summary: "Transfer 40 OMET to 0xf941…12b3",
        by: { index: 2, name: "Bob" },
      },
    ]);
  });

  it("waits for a payload that arrives after its proposal", async () => {
    const { emitted, keeper, set } = harness(undefined);
    await keeper({ type: "proposal", vault: VAULT, id: ID });
    expect(emitted).toEqual([]);
    set(view());
    await keeper({ type: "payload", vault: VAULT, hash: ID });
    expect(emitted.map((e) => e.type)).toEqual(["proposalCreated"]);
  });

  it("names each voter, and announces ready and blocked once", async () => {
    const { emitted, keeper, set } = harness(view());
    await keeper({ type: "proposal", vault: VAULT, id: ID });
    set(view({ status: "ready", votes: [vote(0, "Alice"), vote(1, "Bob")] }));
    await keeper({ type: "approval", vault: VAULT, proposal: ID, slot: 0 });
    await keeper({ type: "approval", vault: VAULT, proposal: ID, slot: 0 });
    expect(emitted.slice(1)).toEqual([
      expect.objectContaining({
        type: "proposalVoted",
        by: { index: 1, name: "Alice" },
        approve: true,
      }),
      expect.objectContaining({ type: "proposalReady", proposal: ID }),
      expect.objectContaining({ type: "proposalVoted", approve: true }),
    ]);

    set(view({ status: "blocked", votes: [vote(1, "Bob"), vote(2, "Carol", "Too large")] }));
    await keeper({ type: "rejection", vault: VAULT, proposal: ID, slot: 2 });
    expect(emitted.slice(4)).toEqual([
      expect.objectContaining({ type: "proposalVoted", approve: false, reason: "Too large" }),
      expect.objectContaining({ type: "proposalBlocked" }),
    ]);
  });

  it("announces who is executing, who executed, and the balance it changed", async () => {
    const tx = `0x${"dd".repeat(32)}` as Hex32;
    const alice = { index: 1, name: "Alice" };
    const { emitted, keeper, set } = harness(view({ status: "ready" }));
    await keeper({ type: "claim", vault: VAULT, proposal: ID, member: 1, phase: "proving" });
    set(
      view({
        status: "executed",
        execution: { txHash: tx, at: 1_900_000_100, executor: alice, block: 12 },
      }),
    );
    await keeper({ type: "receipt", vault: VAULT, proposal: ID, txHash: tx, executor: 1 });
    expect(emitted).toEqual([
      expect.objectContaining({ type: "proposalExecuting", by: alice, phase: "proving" }),
      expect.objectContaining({ type: "proposalExecuted", txHash: tx, by: alice }),
      { type: "balanceChanged", vault: VAULT, token: TOKEN },
    ]);
  });

  it("announces the proposal a cancellation spent", async () => {
    const CANCEL = `0x${"ee".repeat(32)}` as Hex32;
    const tx = `0x${"dd".repeat(32)}` as Hex32;
    const cancel = view({
      id: CANCEL,
      status: "executed",
      intent: { kind: "cancel", cancels: ID, memo: null },
    });
    const { emitted, keeper } = harness(cancel, [view({ status: "cancelled" })]);
    await keeper({ type: "receipt", vault: VAULT, proposal: CANCEL, txHash: tx, executor: 2 });
    expect(emitted).toContainEqual(
      expect.objectContaining({
        type: "proposalCancelled",
        proposal: ID,
        byProposal: CANCEL,
        summary: "Transfer 40 OMET to 0xf941…12b3",
      }),
    );
  });

  it("announces expiring within the hour and expired, once each", async () => {
    const { emitted, set, tick } = harness(view());
    await tick(1_900_000_000);
    expect(emitted).toEqual([]);
    await tick(1_900_083_000);
    await tick(1_900_084_000);
    expect(emitted).toEqual([
      expect.objectContaining({ type: "proposalExpiring", proposal: ID, at: 1_900_086_400 }),
    ]);
    set(view({ status: "expired" }));
    await tick(1_900_090_000);
    await tick(1_900_091_000);
    expect(emitted.slice(1)).toEqual([
      expect.objectContaining({ type: "proposalExpired", proposal: ID }),
    ]);
  });
});
