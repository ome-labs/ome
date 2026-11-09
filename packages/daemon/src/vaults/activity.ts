import { TokenContract, type Transfer } from "@aztec-labs/noir-contracts.js/Token";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { ActivityEntry, Counterparty, Hex32 } from "@ome-labs/api";

import type { Context } from "../context.ts";

/** A private transfer into the vault, as its PXE decrypted the token's `Transfer` event. */
interface Received {
  readonly entry: Extract<ActivityEntry, { kind: "received" }>;
}

const lower = (address: AztecAddress | string): Hex32 => address.toString().toLowerCase();

/**
 * Every private transfer the vault received of the tokens the member tracks, oldest first. The token emits a private
 * `Transfer` event to the recipient; the vault's PXE finds and decrypts it through threshold ECDH like any note. A
 * transfer the vault sent itself (change) is not "received".
 */
export async function receivedTransfers(context: Context, vault: Hex32): Promise<Received[]> {
  const wallet = context.wallet();
  const address = context.vaults.address(vault);
  const tokens = context.state.file.tokens[vault] ?? [];
  const times = new Map<number, number>();
  const at = async (block: number): Promise<number> => {
    const known = times.get(block);
    if (known !== undefined) return known;
    const data = await context.node.node.getBlockData(block as never);
    const time = Number(data?.header.globalVariables.timestamp ?? 0);
    times.set(block, time);
    return time;
  };
  const lists = await Promise.all(
    tokens.map(async (token) => {
      const events = await wallet
        .getPrivateEvents<Transfer>(TokenContract.events.Transfer, {
          contractAddress: AztecAddress.fromStringUnsafe(token.address),
          scopes: [address],
        })
        .catch(() => []);
      const out: Received[] = [];
      for (const { event, metadata } of events) {
        const from = lower(event.from as AztecAddress);
        if (lower(event.to as AztecAddress) !== vault || from === vault) continue;
        const block = Number(metadata.l2BlockNumber);
        out.push({
          entry: {
            kind: "received",
            // oxlint-disable-next-line no-await-in-loop
            at: await at(block),
            token,
            from,
            amount: String(event.amount),
            txHash: metadata.txHash.toString(),
            block,
          },
        });
      }
      return out;
    }),
  );
  return lists.flat().toSorted((a, b) => a.entry.at - b.entry.at);
}

/**
 * Who has paid the vault. A counterparty that paid before a member was removed is `exposed`: the removed member's PXE
 * derived the tagging secret it shares with the vault, so it can still tell when that counterparty pays again (never
 * what or how much) until the counterparty handshakes with the vault anew (`architecture/pxe-integration.md`).
 */
export async function counterparties(context: Context, vault: Hex32): Promise<Counterparty[]> {
  const removals = context.vaults.record(vault).removals ?? [];
  const lastRemoval = Math.max(0, ...removals);
  const contacts = context.state.file.contacts;
  const byAddress = new Map<Hex32, Counterparty>();
  for (const { entry } of await receivedTransfers(context, vault)) {
    const known = byAddress.get(entry.from);
    if (known) {
      known.received += 1;
      known.lastAt = Math.max(known.lastAt, entry.at);
      known.exposed ||= entry.at < lastRemoval;
      continue;
    }
    byAddress.set(entry.from, {
      address: entry.from,
      label: contacts.find((c) => c.address === entry.from)?.label ?? null,
      received: 1,
      firstAt: entry.at,
      lastAt: entry.at,
      exposed: entry.at < lastRemoval,
    });
  }
  return [...byAddress.values()].toSorted((a, b) => b.lastAt - a.lastAt);
}
