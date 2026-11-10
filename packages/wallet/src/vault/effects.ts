// What a simulated vault transaction does to the vault's own notes, read from the simulation without proving.
//
// - Spent: each private call's nullifiers, siloed with its contract, are matched against the vault's notes in this
//   PXE (`siloedNullifier`).
// - Created for the vault: a new note's hash is recomputed with the vault as owner; one the call emitted is the vault's
//   (its change), and any other is someone else's. aztec-nr v6 hashes a note one of two ways:
//   - the `#[note]` macro: `poseidon2([storage_slot, ...note, owner, randomness], NOTE_HASH)`;
//   - `UintNote` (token balances), as a completed partial note: `poseidon2([storage_slot, commitment, value],
//     NOTE_HASH)` with `commitment = poseidon2([owner, randomness], PARTIAL_NOTE_COMMITMENT)`.
import { DomainSeparator } from "@aztec-labs/constants";
import { poseidon2HashWithSeparator } from "@aztec-labs/foundation/crypto/poseidon";
import type { Fr } from "@aztec-labs/foundation/curves/bn254";
import type { PXE } from "@aztec-labs/pxe/server";
import type { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { siloNullifier } from "@aztec-labs/stdlib/hash";
import type {
  NoteAndSlot,
  PrivateCallExecutionResult,
  TxSimulationResult,
} from "@aztec-labs/stdlib/tx";

/** One of the vault's notes, as fields: the contract it belongs to and its packed content. */
export interface VaultNote {
  readonly contract: string;
  readonly items: bigint[];
}

/** What a simulation shows about the vault's notes and the calls' results. */
export interface SimulationFacts {
  /** The vault's notes the transaction spends. */
  readonly spent: VaultNote[];
  /** Notes the transaction creates for the vault. */
  readonly created: VaultNote[];
  /** Note hashes and nullifiers the transaction emits, against Aztec's per-transaction limits. */
  readonly noteHashes: number;
  readonly nullifiers: number;
  /** Return values of the payload's private calls, in order, then of its public calls, in order. */
  readonly privateReturns: bigint[][];
  readonly publicReturns: bigint[][];
  /** Every contract the private calls run, nested ones included, in first-reached order. */
  readonly contracts: string[];
}

/** Every private call of the simulation, depth first. */
function calls(root: PrivateCallExecutionResult): PrivateCallExecutionResult[] {
  return [root, ...root.nestedExecutionResults.flatMap(calls)];
}

const used = <T>(array: { array: readonly T[]; claimedLength: number }) =>
  array.array.slice(0, array.claimedLength);

/** `DOM_SEP__PARTIAL_NOTE_COMMITMENT` in aztec-nr v6 (`aztec/src/note/partial_note.nr`); not a protocol constant. */
const PARTIAL_NOTE_COMMITMENT = 568_912_195;

/** The hashes a note would have if `owner` owned it, one per note shape aztec-nr v6 hashes. */
async function ownedHashes(
  { note, storageSlot, randomness }: NoteAndSlot,
  owner: AztecAddress,
): Promise<string[]> {
  const hashes = [
    await poseidon2HashWithSeparator(
      [storageSlot, ...note.items, owner, randomness],
      DomainSeparator.NOTE_HASH,
    ),
  ];
  const [value] = note.items;
  if (note.items.length === 1 && value) {
    const commitment = await poseidon2HashWithSeparator(
      [owner, randomness],
      PARTIAL_NOTE_COMMITMENT,
    );
    hashes.push(
      await poseidon2HashWithSeparator([storageSlot, commitment, value], DomainSeparator.NOTE_HASH),
    );
  }
  return hashes.map((hash) => hash.toString());
}

export async function simulationFacts(
  pxe: PXE,
  vault: AztecAddress,
  result: TxSimulationResult,
): Promise<SimulationFacts> {
  const all = calls(result.privateExecutionResult.entrypoint);
  const spent: VaultNote[] = [];
  const created: VaultNote[] = [];
  let noteHashes = 0;
  let nullifiers = 0;
  for (const call of all) {
    const contract = call.publicInputs.callContext.contractAddress;
    const emittedHashes = used(call.publicInputs.noteHashes);
    const emittedNullifiers = used(call.publicInputs.nullifiers);
    noteHashes += emittedHashes.length;
    nullifiers += emittedNullifiers.length;

    for (const nullifier of emittedNullifiers) {
      // oxlint-disable-next-line no-await-in-loop
      const siloed = await siloNullifier(contract, nullifier.value);
      // oxlint-disable-next-line no-await-in-loop
      const notes = await pxe.debug.getNotes({
        contractAddress: contract,
        owner: vault,
        siloedNullifier: siloed,
        scopes: [vault],
      });
      for (const note of notes)
        spent.push({
          contract: contract.toString(),
          items: note.note.items.map((f) => f.toBigInt()),
        });
    }

    const hashes = new Set<string>(emittedHashes.map((h) => h.value.toString()));
    for (const note of call.newNotes) {
      // oxlint-disable-next-line no-await-in-loop
      const owned = await ownedHashes(note, vault);
      if (owned.some((hash) => hashes.has(hash)))
        created.push({
          contract: contract.toString(),
          items: note.note.items.map((f: Fr) => f.toBigInt()),
        });
    }
  }
  // The root is the account's entrypoint; its direct children are the payload's private calls, in order.
  const privateReturns = result.privateExecutionResult.entrypoint.nestedExecutionResults.map(
    (call) => call.returnValues.map((f) => f.toBigInt()),
  );
  const publicReturns = result
    .getPublicReturnValues()
    .map((values) => (values.values ?? []).map((f) => f.toBigInt()));
  const contracts = [
    ...new Set(all.map((call) => call.publicInputs.callContext.contractAddress.toString())),
  ];
  return { spent, created, noteHashes, nullifiers, privateReturns, publicReturns, contracts };
}
