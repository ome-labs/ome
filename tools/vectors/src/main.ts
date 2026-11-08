// Generates every golden-vector file from Aztec's own packages. Deterministic for a given Aztec version, except
// Schnorr nonces, which are kept from the committed file while they still verify.
//
//   node src/main.ts           rewrite the vector files
//   node src/main.ts --check   fail if a committed file differs from what the installed Aztec produces

import { header, matches, readCommitted, target, write } from "./files.ts";
import { accountVectors } from "./sets/accounts.ts";
import { contractVectors } from "./sets/contract.ts";
import { curveVectors } from "./sets/curve.ts";
import { poseidon2Vectors } from "./sets/poseidon2.ts";
import { type SchnorrCase, schnorrVectors } from "./sets/schnorr.ts";

interface VectorFile {
  path: string;
  build: (committed: unknown) => Promise<object>;
}

/** Each file a crate's compatibility tests load, and how to build it from its committed version. */
const FILES: VectorFile[] = [
  {
    path: "crates/crypto/tests/vectors/aztec.json",
    build: async (committed) => ({
      ...header,
      ...(await curveVectors()),
      poseidon2: await poseidon2Vectors(),
      schnorr: await schnorrVectors(
        (committed as { schnorr?: SchnorrCase[] } | undefined)?.schnorr,
      ),
      accounts: await accountVectors(),
    }),
  },
  {
    path: "crates/protocol/tests/vectors/contract.json",
    build: async () => ({ ...header, ...(await contractVectors()) }),
  },
];

const check = process.argv.includes("--check");

const results = await Promise.all(
  FILES.map(async ({ path, build }) => {
    const url = target(path);
    const committed = await readCommitted(url);
    return { path, url, committed, vectors: await build(committed) };
  }),
);

if (!check) {
  await Promise.all(results.map(({ url, vectors }) => write(url, vectors)));
  process.stdout.write(results.map(({ path }) => `wrote ${path}\n`).join(""));
}

const stale = check
  ? results.filter(({ committed, vectors }) => !matches(committed, vectors)).map(({ path }) => path)
  : [];

if (stale.length > 0) {
  process.stderr.write(
    `stale golden vectors (run \`pnpm --filter @ome-labs/vectors generate\`):\n${stale.map((p) => `  ${p}\n`).join("")}`,
  );
  process.exit(1);
}
if (check) process.stdout.write("golden vectors up to date\n");
