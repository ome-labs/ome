// One line per proposal for events and notifications, with amounts in whole tokens when the token is known.
import type { ProposalView } from "@ome-labs/api";
import { describe, expect, it } from "vitest";

import { summaryLine, tokenUnits } from "./summary.ts";

const hex = (byte: string) => `0x${byte.repeat(32)}` as const;

describe("summaryLine", () => {
  it("says what each kind of proposal does", () => {
    const transfer = {
      intent: {
        kind: "transfer",
        token: hex("cc"),
        to: hex("11"),
        amount: "75500000000000000000",
        memo: null,
      },
      token: { address: hex("cc"), name: "Ome Test", symbol: "OMET", decimals: 18 },
    } as unknown as ProposalView;
    expect(summaryLine(transfer)).toBe("Transfer 75.5 OMET to 0x1111…1111");
    expect(summaryLine({ ...transfer, token: null })).toBe(
      "Transfer 75500000000000000000 base units of 0xcccc…cccc to 0x1111…1111",
    );
    const calls = (fn: string) =>
      ({
        intent: { kind: "calls", calls: [{ to: hex("22"), fn, args: [] }], memo: null },
      }) as unknown as ProposalView;
    expect(summaryLine(calls("swap"))).toBe("Make 1 contract call");
    expect(summaryLine(calls("rotate_membership"))).toBe("Change the vault's members");
    const swap = calls("swap_exact_tokens_for_tokens");
    const authwit = {
      caller: hex("22"),
      call: { to: hex("cc"), fn: "transfer_to_public", args: [] },
    };
    expect(
      summaryLine({ ...swap, intent: { ...swap.intent, authwits: [authwit] } } as ProposalView),
    ).toBe("Make 1 contract call with 1 authwit");
    expect(
      summaryLine({
        intent: { kind: "cancel", cancels: hex("3e"), memo: null },
      } as unknown as ProposalView),
    ).toBe("Cancel proposal 3e3e");
  });

  it("drops trailing zeros and keeps whole numbers whole", () => {
    expect(tokenUnits("1000000000000000000000", 18)).toBe("1000");
    expect(tokenUnits("1", 18)).toBe("0.000000000000000001");
    expect(tokenUnits("250", 0)).toBe("250");
  });
});
