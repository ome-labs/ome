// Clients follow proposals through the daemon's events: one per proposal that arrives (also once its payload catches up
// with it), one per vote with who cast it, ready and blocked once each, and executed with the balance it touched.
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
  valid: true,
  reason,
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
    approvals: [vote(1, "Bob")],
    rejections: [],
    token: { address: TOKEN, name: "Ome Test", symbol: "OMET", decimals: 18 },
    ...patch,
  } as ProposalView;
}

function harness(initial: ProposalView | undefined) {
  let current = initial;
  const emitted: DaemonEvent[] = [];
  const events = new ProposalEvents(
    (ref: ProposalRef) =>
      current && ref.id === current.id
        ? Promise.resolve(current)
        : Promise.reject(new Error("no payload")),
    (event) => emitted.push(event),
  );
  const keeper = (event: KeeperEvent) => events.onKeeperEvent(event);
  const set = (next: ProposalView | undefined) => {
    current = next;
  };
  return { emitted, keeper, set };
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
    set(view({ status: "ready", approvals: [vote(0, "Alice"), vote(1, "Bob")] }));
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

    set(view({ status: "blocked", rejections: [vote(2, "Carol", "Too large")] }));
    await keeper({ type: "rejection", vault: VAULT, proposal: ID, slot: 2 });
    expect(emitted.slice(4)).toEqual([
      expect.objectContaining({ type: "proposalVoted", approve: false, reason: "Too large" }),
      expect.objectContaining({ type: "proposalBlocked" }),
    ]);
  });

  it("announces an execution and the balance it changed", async () => {
    const tx = `0x${"dd".repeat(32)}` as Hex32;
    const { emitted, keeper } = harness(view({ status: "executed" }));
    await keeper({ type: "receipt", vault: VAULT, proposal: ID, txHash: tx });
    expect(emitted).toEqual([
      expect.objectContaining({ type: "proposalExecuted", txHash: tx }),
      { type: "balanceChanged", vault: VAULT, token: TOKEN },
    ]);
  });
});
