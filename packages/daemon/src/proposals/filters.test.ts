// Filters pick open, ready, done and needs-my-vote proposals; counts summarise them for a badge.
import type { ProposalStatus, ProposalView } from "@ome-labs/api";
import { describe, expect, it } from "vitest";

import { countsOf, matches } from "./filters.ts";

const view = (status: ProposalStatus, voted = false) =>
  ({ status, approvedByMe: voted, rejectedByMe: false }) as ProposalView;

describe("proposal filters", () => {
  const all = [
    view("collecting"),
    view("collecting", true),
    view("ready"),
    view("blocked", true),
    view("executed"),
    view("stale"),
  ];

  const count = (filter: Parameters<typeof matches>[1]) =>
    all.filter((p) => matches(p, filter)).length;

  it("selects by status and by this member's vote", () => {
    expect(count("all")).toBe(6);
    expect(count("open")).toBe(4);
    expect(count("ready")).toBe(1);
    expect(count("done")).toBe(2);
    expect(count("needsMe")).toBe(2);
  });

  it("counts what a badge shows", () => {
    expect(countsOf(all)).toEqual({ open: 4, ready: 1, needsMe: 2 });
  });
});
