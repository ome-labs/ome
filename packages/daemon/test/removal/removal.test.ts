// The member removal drill: the product's promise that a removed member can read nothing that arrives after removal,
// while the vault keeps its address. Carol is removed from a live vault, 2-of-3 with t_v = 2 and again 3-of-4 with
// t_v = 3. Her keeper's state from before the removal is kept and restarted, as a removed member who refuses to forget
// would. Against `aztec start --local-network`.
import { execFileSync, spawn } from "node:child_process";
import { cp, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { EcdhResult, Hex32, Manifest, MemberPartials } from "@ome-labs/api";
import { KeeperClient } from "@ome-labs/wallet";
import { verifyAndCombine } from "@ome-labs/wasm";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { masterKeyStore } from "../../src/secrets/master-key.ts";
import { type Member, PASSPHRASE, found, member, networkAvailable, until } from "../e2e/members.ts";

const available = await networkAvailable();
const crates = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "..", "crates");

/** Starts a keeper on a copy of a data directory, with its master key, and connects to it. */
async function restartKeeper(dir: string, masterKey: Buffer) {
  const token = "drill-token";
  const child = spawn(
    join(crates, "target", "debug", "ome-keeper"),
    ["run", "--data-dir", dir, "--secrets-stdin", "--listen", "/ip4/127.0.0.1/tcp/0"],
    { stdio: ["pipe", "pipe", "ignore"] },
  );
  child.stdin?.end(
    `${JSON.stringify({ masterKey: masterKey.toString("hex"), rpcToken: token })}\n`,
  );
  if (!child.stdout) throw new Error("the keeper has no stdout");
  for await (const line of createInterface({ input: child.stdout })) {
    const ready = (JSON.parse(line) as { ready?: { rpcSocket: string } }).ready;
    if (ready) return { child, client: await KeeperClient.connect(ready.rpcSocket, token) };
  }
  throw new Error("the restarted keeper exited before it was ready");
}

const combine = (
  manifest: Manifest,
  result: { epoch: number; responses: MemberPartials[] },
  inputs: Manifest["groupKey"][],
) =>
  verifyAndCombine({
    epoch: result.epoch,
    preaddress: manifest.preaddress,
    ivpk: manifest.groupKey,
    verificationShares: manifest.members.map((m) => m.verificationShare),
    threshold: manifest.viewingThreshold,
    inputs,
    responses: result.responses,
  });

const SHAPES = [
  { members: 3, threshold: 2, viewingThreshold: 2 },
  { members: 4, threshold: 3, viewingThreshold: 3 },
];

describe.skipIf(!available).each(SHAPES)(
  "removing a member from a $threshold-of-$members vault with t_v = $viewingThreshold",
  ({ members: n, threshold, viewingThreshold }) => {
    let everyone: Member[];
    let carol: Member;
    let staying: Member[];
    let vault: Hex32;
    let token: Hex32;
    let before: Manifest;
    let snapshot: string;
    let openPayment: Hex32;
    const minted = 1000n * 10n ** 18n;

    const balance = async (m: Member) => {
      const [entry] = await m.client.call("tokens.list", { vault }).catch(() => [undefined]);
      return entry?.balance ? BigInt(entry.balance) : undefined;
    };

    beforeAll(async () => {
      execFileSync(
        "cargo",
        ["build", "--manifest-path", join(crates, "Cargo.toml"), "-p", "ome-keeper"],
        {
          stdio: "inherit",
        },
      );
      everyone = await Promise.all(
        ["Alice", "Bob", "Carol", "Dave"].slice(0, n).map((name) => member(name)),
      );
      carol = everyone[2] as Member;
      staying = everyone.filter((m) => m !== carol);
      snapshot = await mkdtemp(join(tmpdir(), "ome-carol-before-"));
    });

    afterAll(async () => {
      await Promise.all(everyone.map((m) => m.stop()));
      await rm(snapshot, { recursive: true, force: true });
    });

    it("founds and funds the vault; Carol approves a payment that stays open", async () => {
      vault = await found(everyone, "Drill", threshold, viewingThreshold);
      const [alice] = everyone as [Member];
      token = (await alice.client.call("tokens.deployTest", { vault })).address;
      for (const m of everyone.slice(1)) {
        // oxlint-disable-next-line no-await-in-loop
        await m.client.call("tokens.add", { vault, token });
        // oxlint-disable-next-line no-await-in-loop
        expect(await until(() => balance(m), `${m.name}'s balance`)).toBe(minted);
      }
      const payment = await alice.client.call("proposals.create", {
        vault,
        intent: { kind: "transfer", token, to: vault, amount: "1", memo: "left open" },
        expiresInHours: 24,
      });
      openPayment = payment.id;
      await until(
        () => carol.client.call("proposals.get", { vault, id: openPayment }).catch(() => undefined),
        "the payment at Carol",
      );
      await carol.client.call("proposals.approve", { vault, id: openPayment });
      before = (await alice.daemon.keeper.client.call("vault.status", { vault })).manifest;
      // What Carol could keep: her keeper's whole state, share included.
      await cp(carol.daemon.paths.keeperDir, snapshot, { recursive: true });
    });

    it("removes Carol: reshare, approvals checked against each member's own result, rotate_membership, activation", async () => {
      const [alice] = everyone as [Member];
      const carolIndex =
        before.members.findIndex((m) => m.identity === carol.daemon.keeper.state.identity) + 1;
      const plan = await alice.client.call("membership.plan", {
        vault,
        remove: [carolIndex],
        add: [],
        threshold: Math.min(threshold, n - 1),
        viewingThreshold,
      });
      expect(plan.failure).toBeNull();
      await Promise.all(
        staying.map((m) => m.client.call("membership.begin", { plan: plan.share })),
      );
      const rotation = await until(async () => {
        const state = await alice.client.call("membership.get", { vault, id: plan.id });
        return state?.proposal ?? undefined;
      }, "the rotation proposal");
      for (const m of staying.slice(0, threshold)) {
        // oxlint-disable-next-line no-await-in-loop
        await until(
          () => m.client.call("proposals.get", { vault, id: rotation }).catch(() => undefined),
          `the rotation at ${m.name}`,
        );
        // oxlint-disable-next-line no-await-in-loop
        await m.client.call("proposals.approve", { vault, id: rotation });
      }
      await until(async () => {
        const view = await alice.client.call("proposals.get", { vault, id: rotation });
        return view.status === "ready" ? true : undefined;
      }, "the rotation to be ready");
      await alice.client.call("proposals.execute", { vault, id: rotation });
      for (const m of staying) {
        // oxlint-disable-next-line no-await-in-loop
        const done = await until(async () => {
          const state = await m.client.call("membership.get", { vault, id: plan.id });
          return state?.done ? state : undefined;
        }, `${m.name}'s rotation`);
        expect(done.error).toBeNull();
      }
    });

    it("a payment that arrives after the removal is read by every member who stays", async () => {
      const [alice] = everyone as [Member];
      const account = AztecAddress.fromStringUnsafe(
        (await alice.client.call("status", {})).profile?.account as Hex32,
      );
      await TokenContract.at(AztecAddress.fromStringUnsafe(token), alice.daemon.wallet())
        .methods.mint_to_private(AztecAddress.fromStringUnsafe(vault), 5n)
        .send({ from: account, fee: alice.daemon.fee() });
      for (const m of staying) {
        // oxlint-disable-next-line no-await-in-loop
        await until(
          async () => ((await balance(m)) === minted + 5n ? true : undefined),
          `${m.name}'s new balance`,
        );
      }
    });

    it("Carol's old keeper state cannot read it: its partials and the new committee's never combine", async () => {
      const [alice] = everyone as [Member];
      const after = (await alice.daemon.keeper.client.call("vault.status", { vault })).manifest;
      expect(after.vault).toBe(before.vault);
      expect(after.epoch).toBeGreaterThan(before.epoch);
      // The reshare refreshed every share: no verification share of the new committee is an old one.
      const old = new Set(
        before.members.map((m) => `${m.verificationShare.x}:${m.verificationShare.y}`),
      );
      expect(
        after.members.some((m) => old.has(`${m.verificationShare.x}:${m.verificationShare.y}`)),
      ).toBe(false);

      const point = after.groupKey;
      const fresh = (await alice.daemon.keeper.client.call("ecdh.batch", {
        vault,
        epoch: after.epoch,
        purpose: "decryption",
        points: [point],
      })) as EcdhResult;
      expect(combine(after, fresh, [point]).points).toHaveLength(1);
      // The new committee's partials do not verify as partials of the old committee, so a share of the old one
      // cannot complete a combination with them.
      expect(() => combine(before, { ...fresh, epoch: before.epoch }, [point])).toThrow();

      // Carol's keeper restarted from before the removal: the members who stay no longer answer its old epoch.
      const sealing = (await carol.client.call("signer.sealingKey", { passphrase: PASSPHRASE }))
        .key;
      const key = await (
        await masterKeyStore("drill", carol.daemon.paths.masterKey)
      ).read(Buffer.from(sealing.slice(2), "hex"));
      if (!key) throw new Error("Carol's master key did not open");
      const restarted = await restartKeeper(snapshot, key);
      try {
        await expect(
          restarted.client.call("ecdh.batch", {
            vault,
            epoch: before.epoch,
            purpose: "decryption",
            points: [point],
          }),
        ).rejects.toThrow();
      } finally {
        restarted.client.close();
        restarted.child.kill();
      }
    });

    it("Carol's approval no longer counts, and her live daemon sees only what it knew before", async () => {
      const [alice] = everyone as [Member];
      const open = await alice.client.call("proposals.get", { vault, id: openPayment });
      expect(open.status).toBe("stale");
      await expect(
        alice.client.call("proposals.execute", { vault, id: openPayment }),
      ).rejects.toThrow();
      const known = await balance(carol);
      expect(known === undefined || known !== minted + 5n).toBe(true);
      expect(await carol.client.call("vault.retire", { vault })).toEqual([]);
      expect((await alice.client.call("vault.list", {})).map((v) => v.address)).toContain(vault);
    });
  },
);
