// Announces transfers into a vault as its PXE finds them. The first look at a vault only learns what it already
// received, so a restart doesn't announce old payments again.
import type { DaemonEvent, Hex32 } from "@ome-labs/api";

import type { Context } from "../context.ts";
import { receivedTransfers } from "./activity.ts";

type Entry = Awaited<ReturnType<typeof receivedTransfers>>[number]["entry"];

const idOf = (entry: Entry) =>
  `${entry.txHash}:${entry.token.address}:${entry.from}:${entry.amount}`;

export class ReceivedWatcher {
  readonly #context: Context;
  /** Per vault, every transfer already seen, by transaction, token, sender and amount. */
  readonly #seen = new Map<Hex32, Set<string>>();

  constructor(context: Context) {
    this.#context = context;
  }

  /** Looks for new transfers into `vault`; a vault that can't be read now is looked at again on the next sync. */
  async check(vault: Hex32): Promise<void> {
    const received = await receivedTransfers(this.#context, vault).catch(() => undefined);
    if (!received) return;
    const seen = this.#seen.get(vault);
    this.#seen.set(vault, new Set(received.map(({ entry }) => idOf(entry))));
    if (!seen) return;
    const tokens = new Set<Hex32>();
    for (const { entry } of received) {
      if (seen.has(idOf(entry))) continue;
      const event: DaemonEvent = {
        type: "received",
        vault,
        token: entry.token,
        amount: entry.amount,
        from: entry.from,
        txHash: entry.txHash,
      };
      this.#context.emit(event);
      tokens.add(entry.token.address);
    }
    for (const token of tokens) this.#context.emit({ type: "balanceChanged", vault, token });
  }
}
