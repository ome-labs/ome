// Times Ome with real client-side proofs against `aztec start --local-network`: three daemons with the prover on found
// a 2-of-3 vault (the deployment proven), pay from it, and remove a member (`rotate_membership` proven). It also times
// threshold reads (direct, through the members' keepers) and DKGs among 3 and 8 keepers. Prints a table and JSON.
//
//   pnpm bench:proving      (from the repository root; takes several minutes)

import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import type { DkgResult, Hex32, KeeperContact } from "@ome-labs/api";
import { KeeperClient } from "@ome-labs/wallet";

import { type Member, found, member, networkAvailable, until } from "../test/e2e/members.ts";

const crates = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "crates");
const results: Record<string, number> = {};

async function timed<T>(name: string, run: () => Promise<T>): Promise<T> {
  const start = performance.now();
  const value = await run();
  results[name] = Math.round(performance.now() - start);
  console.log(`${name.padEnd(48)} ${(results[name] / 1000).toFixed(2)} s`);
  return value;
}

/** Starts a bare keeper on loopback and connects to it. */
async function keeper(dir: string) {
  const token = "bench-token";
  const child = spawn(
    join(crates, "target", "debug", "ome-keeper"),
    ["run", "--data-dir", dir, "--secrets-stdin", "--listen", "/ip4/127.0.0.1/tcp/0"],
    { stdio: ["pipe", "pipe", "ignore"] },
  );
  child.stdin?.end(`${JSON.stringify({ masterKey: "11".repeat(32), rpcToken: token })}\n`);
  if (!child.stdout) throw new Error("the keeper has no stdout");
  for await (const line of createInterface({ input: child.stdout })) {
    const ready = (JSON.parse(line) as { ready?: { rpcSocket: string } }).ready;
    if (ready) return { child, client: await KeeperClient.connect(ready.rpcSocket, token) };
  }
  throw new Error("a keeper exited before it was ready");
}

/** A DKG among `n` bare keepers with threshold `t`. */
async function dkg(n: number, t: number): Promise<void> {
  const home = await mkdtemp(join(tmpdir(), "ome-bench-dkg-"));
  const keepers = await Promise.all(
    Array.from({ length: n }, (_, i) => keeper(join(home, `k${i}`))),
  );
  try {
    const members: KeeperContact[] = await Promise.all(
      keepers.map(async ({ client }) => {
        const info = await client.call("keeper.info", {});
        return { identity: info.identity, transport: info.transport, addresses: info.addresses };
      }),
    );
    const session =
      `0x${Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString("hex")}` as Hex32;
    await timed(`DKG, ${n} keepers, t_v = ${t}`, async () => {
      await Promise.all(
        keepers.map(({ client }) =>
          client.call("ceremony.dkg", { session, threshold: t, members }),
        ),
      );
      await Promise.all(
        keepers.map(({ client }) =>
          until(async () => {
            const ceremony = await client.call("ceremony.status", { session });
            return ceremony.status === "completed" ? (ceremony.result as DkgResult) : undefined;
          }, "the DKG"),
        ),
      );
    });
  } finally {
    for (const { client, child } of keepers) {
      client.close();
      child.kill();
    }
    await rm(home, { recursive: true, force: true });
  }
}

if (!(await networkAvailable())) {
  console.error("no Aztec node: run `aztec start --local-network` first");
  process.exit(1);
}

const members: Member[] = await Promise.all(
  ["Alice", "Bob", "Carol"].map((name) => member(name, { proverEnabled: true })),
);
try {
  const [alice, bob, carol] = members as [Member, Member, Member];
  const vault = await timed("found a 2-of-3 vault (DKG and proven deployment)", () =>
    found(members, "Bench", 2, 2),
  );
  const token = await timed(
    "deploy a test token and mint to the vault (proven)",
    async () => (await alice.client.call("tokens.deployTest", { vault })).address,
  );
  await bob.client.call("tokens.add", { vault, token });
  await until(async () => {
    const [entry] = await bob.client.call("tokens.list", { vault });
    return entry?.balance ? true : undefined;
  }, "Bob's balance");

  const bobAccount = (await bob.client.call("status", {})).profile?.account as Hex32;
  const payment = await alice.client.call("proposals.create", {
    vault,
    intent: { kind: "transfer", token, to: bobAccount, amount: "1", memo: null },
    expiresInHours: 1,
  });
  await until(
    () => bob.client.call("proposals.get", { vault, id: payment.id }).catch(() => undefined),
    "the payment",
  );
  await Promise.all(
    [alice, bob].map((m) => m.client.call("proposals.approve", { vault, id: payment.id })),
  );
  await timed("2-of-3 payment: rebuild, prove, send, mined", () =>
    alice.client.call("proposals.execute", { vault, id: payment.id }),
  );

  const manifest = (await alice.daemon.keeper.client.call("vault.status", { vault })).manifest;
  for (const count of [1, 100]) {
    const points = Array.from({ length: count }, () => manifest.groupKey);
    const runs = 5;
    const start = performance.now();
    for (let i = 0; i < runs; i += 1)
      // oxlint-disable-next-line no-await-in-loop
      await alice.daemon.keeper.client.call("ecdh.batch", {
        vault,
        epoch: manifest.epoch,
        purpose: "decryption",
        points,
      });
    results[`threshold read, ${count} point(s), direct`] = Math.round(
      (performance.now() - start) / runs,
    );
    console.log(
      `${`threshold read, ${count} point(s), direct`.padEnd(48)} ${results[`threshold read, ${count} point(s), direct`]} ms`,
    );
  }

  const carolIndex =
    manifest.members.findIndex((m) => m.identity === carol.daemon.keeper.state.identity) + 1;
  const plan = await alice.client.call("membership.plan", {
    vault,
    remove: [carolIndex],
    add: [],
    threshold: 2,
    viewingThreshold: 2,
  });
  await Promise.all(
    [alice, bob].map((m) => m.client.call("membership.begin", { plan: plan.share })),
  );
  const rotation = await until(async () => {
    const state = await alice.client.call("membership.get", { vault, id: plan.id });
    return state?.proposal ?? undefined;
  }, "the rotation proposal");
  await until(
    () => bob.client.call("proposals.get", { vault, id: rotation }).catch(() => undefined),
    "the rotation at Bob",
  );
  await Promise.all(
    [alice, bob].map((m) => m.client.call("proposals.approve", { vault, id: rotation })),
  );
  await timed("remove a member: rotate_membership proven and mined", () =>
    alice.client.call("proposals.execute", { vault, id: rotation }),
  );
} finally {
  await Promise.all(members.map((m) => m.stop()));
}

await dkg(3, 2);
await dkg(8, 5);
console.log(JSON.stringify(results, null, 2));
// The PXEs keep timers alive after their daemons stop.
process.exit(0);
