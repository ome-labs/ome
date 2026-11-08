// Poseidon2 as Aztec uses it: the raw permutation, plain hashes of several lengths, and hashes with a domain
// separator.

import { DomainSeparator } from "@aztec-labs/constants";
import {
  poseidon2Hash,
  poseidon2HashWithSeparator,
  poseidon2Permutation,
} from "@aztec-labs/foundation/crypto/poseidon";
import { Fr } from "@aztec-labs/foundation/curves/bn254";

import { hex } from "../encoding.ts";

const toFields = (values: bigint[]): Fr[] => values.map((value) => new Fr(value));

const HASH_INPUTS: bigint[][] = [
  [1n],
  [1n, 2n],
  [1n, 2n, 3n],
  [1n, 2n, 3n, 4n],
  [7n, 8n, 9n, 10n, 11n],
];

const SEPARATED_INPUTS: [number, bigint[]][] = [
  [DomainSeparator.CONTRACT_ADDRESS_V3, [5n, 6n]],
  [DomainSeparator.APP_SILOED_ECDH_SHARED_SECRET, [1n, 2n, 3n]],
];

interface HashCase {
  input: string[];
  output: string;
}

export interface Poseidon2Vectors {
  permutation: { input: string[]; output: string[] };
  hashes: HashCase[];
  separated: (HashCase & { separator: number })[];
}

export async function poseidon2Vectors(): Promise<Poseidon2Vectors> {
  const state = toFields([0n, 1n, 2n, 3n]);

  return {
    permutation: {
      input: state.map(hex),
      output: (await poseidon2Permutation(state)).map(hex),
    },
    hashes: await Promise.all(
      HASH_INPUTS.map(async (values) => ({
        input: toFields(values).map(hex),
        output: hex(await poseidon2Hash(toFields(values))),
      })),
    ),
    separated: await Promise.all(
      SEPARATED_INPUTS.map(async ([separator, values]) => ({
        separator,
        input: toFields(values).map(hex),
        output: hex(await poseidon2HashWithSeparator(toFields(values), separator)),
      })),
    ),
  };
}
