// Generates every golden-vector file from Aztec's own packages. Deterministic for a given Aztec version, except
// Schnorr nonces, which are kept from the committed file while they still verify.
//
//   node src/main.mjs           rewrite the vector files
//   node src/main.mjs --check   fail if a committed file differs from what the installed Aztec produces

import { header, matches, readCommitted, target, write } from "./files.mjs";
import { accountVectors } from "./sets/accounts.mjs";
import { contractVectors } from "./sets/contract.mjs";
import { curveVectors } from "./sets/curve.mjs";
import { poseidon2Vectors } from "./sets/poseidon2.mjs";
import { schnorrVectors } from "./sets/schnorr.mjs";

/** Each file a crate's compatibility tests load, and how to build it from its committed version. */
const FILES = [
  {
    path: "crates/crypto/tests/vectors/aztec.json",
    build: async (committed) => ({
      ...header,
      ...(await curveVectors()),
      poseidon2: await poseidon2Vectors(),
      schnorr: await schnorrVectors(committed?.schnorr),
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
