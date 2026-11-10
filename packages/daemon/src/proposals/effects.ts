// A simulation's effects as a review shows them: each tracked token's vault balance before and after, the calls'
// decoded return values, and the notes it creates and spends.
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { type FunctionCall, FunctionType, decodeFromAbi } from "@aztec-labs/stdlib/abi";
import type { BalanceChange, CallResult, Effects, Hex32, TokenInfo } from "@ome-labs/api";
import type { SimulationFacts, VaultNote } from "@ome-labs/wallet";

import type { Context } from "../context.ts";
import { apiType, apiValue } from "../contracts/abi.ts";
import { balances } from "../vaults/tokens.ts";

const lower = (text: string) => text.toLowerCase() as Hex32;

/** The value a note carries, for the note shape a token's balance uses (`UintNote`: one field). */
function valueOf(note: VaultNote): bigint | undefined {
  return note.items.length === 1 ? note.items[0] : undefined;
}

/**
 * Effects from `facts` for `vault`, whose tracked `tokens` give names and decimals. `complete` is false when a note
 * of the vault changed that this can't value: an untracked contract's, or one of another shape. The vault's own notes
 * (its config and signers, which governance calls replace) hold no balance and are left out.
 */
export async function effectsOf(
  context: Context,
  vault: Hex32,
  calls: readonly FunctionCall[],
  facts: SimulationFacts,
  tokens: readonly TokenInfo[],
): Promise<Effects> {
  let complete = true;
  const deltas = new Map<Hex32, bigint>();
  const apply = (note: VaultNote, sign: 1n | -1n) => {
    const contract = lower(note.contract);
    if (contract === vault) return;
    const amount = valueOf(note);
    if (!tokens.some((t) => t.address === contract) || amount === undefined) {
      complete = false;
      return;
    }
    deltas.set(contract, (deltas.get(contract) ?? 0n) + sign * amount);
  };
  for (const note of facts.spent) apply(note, -1n);
  for (const note of facts.created) apply(note, 1n);

  const changed = tokens.filter((t) => (deltas.get(t.address) ?? 0n) !== 0n);
  const before = changed.length > 0 ? await balances(context, vault, changed) : [];
  const changes: BalanceChange[] = [];
  for (const read of before) {
    if (read.balance === null) {
      // Without the balance before, the change can't be shown as before and after.
      complete = false;
      continue;
    }
    const delta = deltas.get(read.token.address) ?? 0n;
    changes.push({
      token: read.token,
      before: read.balance,
      after: (BigInt(read.balance) + delta).toString(),
      visibility: "private",
    });
  }

  return {
    balances: changes,
    returns: await returnsOf(context, calls, facts),
    notes: { created: facts.noteHashes, nullified: facts.nullifiers },
    complete,
  };
}

/** Each call's return values, decoded with its function's return type when this member has the artifact. */
async function returnsOf(
  context: Context,
  calls: readonly FunctionCall[],
  facts: SimulationFacts,
): Promise<CallResult[]> {
  let nextPrivate = 0;
  let nextPublic = 0;
  const results: CallResult[] = [];
  for (const [index, call] of calls.entries()) {
    const raw =
      call.type === FunctionType.PRIVATE
        ? facts.privateReturns[nextPrivate++]
        : facts.publicReturns[nextPublic++];
    if (!raw || raw.length === 0) continue;
    // oxlint-disable-next-line no-await-in-loop
    const found = await context.contracts
      .artifact(lower(call.to.toString()))
      .catch(() => undefined);
    const fn = found?.artifact.functions.find((f) => f.name === call.name);
    const types = fn?.returnTypes ?? [];
    const fields = raw.map((field) => new Fr(field));
    results.push({
      call: index,
      returns:
        types.length === 1 && types[0]
          ? [
              {
                name: "return",
                type: apiType(types[0]),
                value: apiValue(decodeFromAbi(types[0], fields)),
                display: null,
              },
            ]
          : raw.map((field, i) => ({
              name: `return${i}`,
              type: { kind: "field" as const },
              value: field.toString(),
              display: null,
            })),
    });
  }
  return results;
}
