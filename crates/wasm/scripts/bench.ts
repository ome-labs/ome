// Times the hot paths of @ome-labs/wasm in Node, for the progress notes and to compare with native Rust:
// verify-and-combine per point (every note read), the outer hash, and an Argon2id unseal.
//
//   node scripts/bench.ts   (after `pnpm --filter @ome-labs/wasm build`)

import * as wasm from "@ome-labs/wasm";
import * as testing from "@ome-labs/wasm/testing";

const field = (value: number): string => `0x${value.toString(16).padStart(64, "0")}`;

function time(label: string, runs: number, run: () => unknown): void {
  run();
  const start = performance.now();
  for (let i = 0; i < runs; i += 1) run();
  const each = (performance.now() - start) / runs;
  console.log(`${label.padEnd(44)} ${each.toFixed(3)} ms`);
}

const committee = testing.dealShares(field(7), 2, 3);
const preaddress = field(9);
const ivpk = committee.groupKey;
const points = (count: number) =>
  Array.from({ length: count }, (_, i) => testing.publicKey(field(i + 11)));

for (const count of [1, 100, 1000]) {
  const inputs = points(count);
  const responses = [0, 1].map((i) =>
    testing.partials(i + 1, committee.shares[i] ?? "", preaddress, ivpk, 0, inputs),
  );
  const request = {
    epoch: 0,
    preaddress,
    ivpk,
    verificationShares: committee.verificationShares,
    threshold: 2,
    inputs,
    responses,
  };
  time(`verifyAndCombine, t_v = 2, ${count} point(s)`, count === 1000 ? 3 : 10, () =>
    wasm.verifyAndCombine(request),
  );
  if (count === 1) {
    time("partials (one member, one point)", 50, () =>
      testing.partials(1, committee.shares[0] ?? "", preaddress, ivpk, 0, inputs),
    );
  }
}

const call = {
  signature: "transfer_private_to_private((Field),(Field),u128,Field)",
  to: field(1),
  selector: wasm.selector("transfer_private_to_private((Field),(Field),u128,Field)"),
  kind: "private" as const,
  isStatic: false,
  hideMsgSender: false,
  args: [field(2), field(3), field(4), field(5)],
};
time("outerHash, 5 calls", 200, () =>
  wasm.outerHash(field(1), { chainId: field(2), version: field(1) }, field(3), [
    call,
    call,
    call,
    call,
    call,
  ]),
);

const sealed = wasm.generateSigner({ passphrase: "correct horse battery" });
time("UnlockedSigner.open, Argon2id 64 MiB x 3", 5, () =>
  wasm.UnlockedSigner.open(sealed, { passphrase: "correct horse battery" }).free(),
);
const key = wasm.sealingKey(sealed, "correct horse battery");
time("UnlockedSigner.open with the sealing key", 50, () =>
  wasm.UnlockedSigner.open(sealed, { key }).free(),
);
