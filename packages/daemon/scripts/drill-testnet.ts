// The first run on a real network: three members' daemons found a 2-of-3 vault on Aztec Testnet, fund it, pay from it
// and remove a member, with real client-side proofs and Testnet's sponsored fee payer (the canonical `SponsoredFPC`,
// which the daemon's default fee payer salt derives). Every step is timed. Needs only network access.
//
//   pnpm drill:testnet                                   (OME_TESTNET_URL overrides https://aztec-testnet.drpc.org)

import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { Hex32 } from "@ome-labs/api";

import { type Member, found, member, until } from "../test/e2e/members.ts";

const NODE = process.env.OME_TESTNET_URL ?? "https://aztec-testnet.drpc.org";
const STEP_MS = 30 * 60 * 1000;
const steps: { step: string; seconds: number }[] = [];

async function step<T>(name: string, run: () => Promise<T>): Promise<T> {
  console.log(`… ${name}`);
  const start = performance.now();
  const value = await run();
  const seconds = Math.round((performance.now() - start) / 1000);
  steps.push({ step: name, seconds });
  console.log(`✓ ${name} (${seconds} s)`);
  return value;
}

const info = (await (
  await fetch(NODE, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "node_getNodeInfo", params: [] }),
  })
).json()) as { result?: { nodeVersion: string; l1ChainId: number; rollupVersion: number } };
if (!info.result) throw new Error(`no Aztec node at ${NODE}`);
console.log(
  `Testnet: Aztec ${info.result.nodeVersion}, L1 ${info.result.l1ChainId}, rollup ${info.result.rollupVersion}`,
);
if (info.result.nodeVersion !== "6.0.0") throw new Error("the drill is pinned to Aztec 6.0.0");

const members: Member[] = await step("start three daemons (prover on)", () =>
  Promise.all(
    ["Alice", "Bob", "Carol"].map((name) => member(name, { nodeUrl: NODE, proverEnabled: true })),
  ),
);
try {
  const [alice, bob, carol] = members as [Member, Member, Member];
  const vault = await step("found a 2-of-3 vault (DKG, proven deployment)", () =>
    found(members, "Drill", 2, 2),
  );
  console.log(`  vault ${vault}`);

  const aliceAccount = AztecAddress.fromStringUnsafe(
    (await alice.client.call("status", {})).profile?.account as Hex32,
  );
  const token = await step(
    "deploy a token from Alice's account and mint to the vault",
    async () => {
      const fee = alice.daemon.fee();
      const { contract } = await TokenContract.deploy(
        alice.daemon.wallet(),
        aliceAccount,
        "Ome Drill",
        "DRILL",
        18,
      ).send({
        from: aliceAccount,
        fee,
      });
      await contract.methods
        .mint_to_private(AztecAddress.fromStringUnsafe(vault), 1000n)
        .send({ from: aliceAccount, fee });
      return contract.address.toString() as Hex32;
    },
  );
  await step("every member reads the vault's balance through threshold ECDH", async () => {
    for (const m of members) {
      // oxlint-disable-next-line no-await-in-loop
      await m.client.call("tokens.add", { vault, token });
      // oxlint-disable-next-line no-await-in-loop
      await until(
        async () => {
          const [entry] = await m.client.call("tokens.list", { vault });
          return entry?.balance === "1000" ? true : undefined;
        },
        `${m.name}'s balance`,
        STEP_MS,
        5000,
      );
    }
  });

  const bobAccount = (await bob.client.call("status", {})).profile?.account as Hex32;
  await step("pay Bob with two approvals (proven)", async () => {
    const payment = await alice.client.call("proposals.create", {
      vault,
      intent: { kind: "transfer", token, to: bobAccount, amount: "10", memo: "drill" },
      expiresInHours: 6,
    });
    await until(
      () => bob.client.call("proposals.get", { vault, id: payment.id }).catch(() => undefined),
      "the payment at Bob",
      STEP_MS,
      2000,
    );
    await Promise.all(
      [alice, bob].map((m) => m.client.call("proposals.approve", { vault, id: payment.id })),
    );
    const executed = await alice.client.call("proposals.execute", { vault, id: payment.id });
    if (executed.status !== "executed") throw new Error(`the payment is ${executed.status}`);
  });

  await step("remove Carol: reshare and rotate_membership (proven)", async () => {
    const manifest = (await alice.daemon.keeper.client.call("vault.status", { vault })).manifest;
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
    const rotation = await until(
      async () => {
        const state = await alice.client.call("membership.get", { vault, id: plan.id });
        return state?.proposal ?? undefined;
      },
      "the rotation proposal",
      STEP_MS,
      2000,
    );
    await until(
      () => bob.client.call("proposals.get", { vault, id: rotation }).catch(() => undefined),
      "the rotation at Bob",
      STEP_MS,
      2000,
    );
    await Promise.all(
      [alice, bob].map((m) => m.client.call("proposals.approve", { vault, id: rotation })),
    );
    await alice.client.call("proposals.execute", { vault, id: rotation });
    for (const m of [alice, bob])
      // oxlint-disable-next-line no-await-in-loop
      await until(
        async () => {
          const state = await m.client.call("membership.get", { vault, id: plan.id });
          if (state?.error) throw new Error(state.error);
          return state?.done ? true : undefined;
        },
        `${m.name}'s rotation`,
        STEP_MS,
        5000,
      );
  });

  await step("the members who stay still read the vault", async () => {
    await until(
      async () => {
        const [entry] = await bob.client.call("tokens.list", { vault });
        return entry?.balance === "990" ? true : undefined;
      },
      "Bob's balance after the rotation",
      STEP_MS,
      5000,
    );
  });
} finally {
  await Promise.all(members.map((m) => m.stop()));
}

console.log("\nTestnet drill");
for (const { step: name, seconds } of steps) console.log(`  ${name.padEnd(60)} ${seconds} s`);
