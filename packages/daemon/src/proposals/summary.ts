// One line saying what a proposal does, for events and notifications: "Transfer 40 OMET to 0xf941…12b3".
import type { Amount, ProposalView } from "@ome-labs/api";

const ROTATE = "rotate_membership";

/** `0xf941…12b3`. */
export function shortHex(hex: string): string {
  return `${hex.slice(0, 6)}…${hex.slice(-4)}`;
}

/** A base-unit amount in whole tokens, without trailing zeros: `75500000000000000000`, 18 → `75.5`. */
export function tokenUnits(amount: Amount, decimals: number): string {
  const units = BigInt(amount);
  const scale = 10n ** BigInt(decimals);
  const whole = units / scale;
  const fraction = (units % scale).toString().padStart(decimals, "0").replace(/0+$/u, "");
  return fraction ? `${whole}.${fraction}` : whole.toString();
}

export function summaryLine(view: ProposalView): string {
  const { intent, token } = view;
  switch (intent.kind) {
    case "transfer": {
      // A token this member doesn't track has no known decimals, so base units are the only honest number.
      const amount = token
        ? `${tokenUnits(intent.amount, token.decimals)} ${token.symbol}`
        : `${intent.amount} base units of ${shortHex(intent.token)}`;
      return `Transfer ${amount} to ${shortHex(intent.to)}`;
    }
    case "calls": {
      if (intent.calls.some((call) => call.fn === ROTATE)) return "Change the vault's members";
      const count = intent.calls.length;
      const calls = `Make ${count} contract ${count === 1 ? "call" : "calls"}`;
      const granted = intent.authwits?.length ?? 0;
      return granted > 0
        ? `${calls} with ${granted} ${granted === 1 ? "authwit" : "authwits"}`
        : calls;
    }
    case "cancel":
      return `Cancel proposal ${intent.cancels.slice(2, 6)}`;
    case "setThreshold":
      return `Need ${intent.threshold} ${intent.threshold === 1 ? "approval" : "approvals"} to spend`;
    case "invalidatePending":
      return "Void every approval collected so far";
    case "app":
      return `A transaction from ${intent.name ?? intent.origin}`;
    case "unknown":
      return "A proposal from a newer release of Ome";
  }
}
