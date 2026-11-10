// Which proposals a list shows, and a vault's open proposals at a glance.
import type { ProposalCounts, ProposalFilter, ProposalStatus, ProposalView } from "@ome-labs/api";

const OPEN: ReadonlySet<ProposalStatus> = new Set(["collecting", "ready", "blocked"]);
const DONE: ReadonlySet<ProposalStatus> = new Set(["executed", "cancelled", "expired", "stale"]);

/** Collecting or ready, and this member hasn't voted: a vote on a blocked proposal changes nothing. */
const needsMe = (p: ProposalView) =>
  (p.status === "collecting" || p.status === "ready") && !p.approvedByMe && !p.rejectedByMe;

/** Whether `p` passes `filter`. */
export function matches(p: ProposalView, filter: ProposalFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "open":
      return OPEN.has(p.status);
    case "ready":
      return p.status === "ready";
    case "done":
      return DONE.has(p.status);
    case "needsMe":
      return needsMe(p);
    default:
      return true;
  }
}

/** How many of `proposals` are open, ready, and waiting for this member's vote. */
export function countsOf(proposals: readonly ProposalView[]): ProposalCounts {
  return {
    open: proposals.filter((p) => OPEN.has(p.status)).length,
    ready: proposals.filter((p) => p.status === "ready").length,
    needsMe: proposals.filter(needsMe).length,
  };
}
