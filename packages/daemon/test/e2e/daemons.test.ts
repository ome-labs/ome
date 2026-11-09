// Three members found a 2-of-3 vault through their daemons, pay from it with two approvals, see a rejection block a
// proposal, and then rotate: Carol leaves and a newcomer joins with a join request, receiving the vault's protocol
// secrets in the reshare. Everything goes through the daemons' sockets, as an app would; each daemon runs its own
// keeper process and PXE against `aztec start --local-network`.
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { openStoreAt } from "@aztec-labs/kv-store/lmdb-v2";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { FoundingState, Hex32, ProposalView, RotationState } from "@ome-labs/api";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { type Member, member, networkAvailable, until } from "./members.ts";

const available = await networkAvailable();

describe.skipIf(!available)("members' daemons on a local network", () => {
  let alice: Member; // member 1: founds and proposes
  let bob: Member; // member 2
  let carol: Member; // leaves in the rotation
  let dave: Member; // joins in the rotation
  let vault: Hex32;
  let token: Hex32;

  const proposal = (m: Member, id: Hex32) =>
    until(
      () => m.client.call("proposals.get", { vault, id }).catch(() => undefined),
      `proposal ${id} at ${m.name}`,
    );

  const status = async (m: Member, id: Hex32, wanted: ProposalView["status"]) =>
    until(async () => {
      const view = await m.client.call("proposals.get", { vault, id }).catch(() => undefined);
      return view?.status === wanted ? view : undefined;
    }, `proposal ${id} to be ${wanted} at ${m.name}`);

  const balance = async (m: Member) => {
    const [entry] = await m.client.call("tokens.list", { vault });
    return entry?.balance === null || entry?.balance === undefined
      ? undefined
      : BigInt(entry.balance);
  };

  beforeAll(async () => {
    [alice, bob, carol] = await Promise.all([member("Alice"), member("Bob"), member("Carol")]);
  });

  afterAll(async () => {
    await Promise.all([alice, bob, carol, dave].map((m) => m?.stop()));
  });

  describe("founding", () => {
    it("three daemons found a vault through a lobby and agree on its address", async () => {
      const lobby = await alice.client.call("lobby.create", {
        name: "Treasury",
        threshold: 2,
        viewingThreshold: 2,
      });
      expect(lobby.invite).toMatch(/^ome-invite-v0:/u);
      const preview = await bob.client.call("lobby.inspect", { invite: lobby.invite ?? "" });
      expect(preview.sameNetwork).toBe(true);
      await Promise.all(
        [bob, carol].map((m) => m.client.call("lobby.join", { invite: lobby.invite ?? "" })),
      );
      await until(async () => {
        const view = await alice.client.call("lobby.get", { lobby: lobby.lobby });
        return view.members.length === 3 ? view : undefined;
      }, "both guests to join");
      await alice.client.call("lobby.start", {
        lobby: lobby.lobby,
        threshold: 2,
        viewingThreshold: 2,
      });

      const states = await Promise.all(
        [alice, bob, carol].map(async (m) => {
          await m.client.call("founding.begin", { lobby: lobby.lobby });
          return until(async () => {
            const state = await m.client.call("founding.get", { lobby: lobby.lobby });
            return state?.done ? state : undefined;
          }, `${m.name}'s founding`);
        }),
      );
      for (const state of states as FoundingState[]) expect(state.error).toBeNull();
      const addresses = new Set(states.map((s) => s.vault));
      expect(addresses.size).toBe(1);
      vault = states[0]?.vault as Hex32;
      for (const m of [alice, bob, carol]) {
        // oxlint-disable-next-line no-await-in-loop
        const [summary] = await m.client.call("vault.list", {});
        expect(summary).toMatchObject({
          address: vault,
          threshold: 2,
          viewingThreshold: 2,
          members: 3,
        });
      }
    });
  });

  describe("payments", () => {
    it("pays with two approvals; every member's daemon sees the vault's balance", async () => {
      token = (await alice.client.call("tokens.deployTest", { vault })).address;
      await Promise.all([bob, carol].map((m) => m.client.call("tokens.add", { vault, token })));
      const minted = 1000n * 10n ** 18n;
      for (const m of [alice, bob, carol]) {
        // oxlint-disable-next-line no-await-in-loop
        expect(await until(() => balance(m), `${m.name}'s balance`)).toBe(minted);
      }

      const bobAccount = (await bob.client.call("status", {})).profile?.account as Hex32;
      const created = await alice.client.call("proposals.create", {
        vault,
        intent: { kind: "transfer", token, to: bobAccount, amount: "100", memo: "pay Bob" },
        expiresInHours: 1,
      });
      expect(created.status).toBe("collecting");

      // Bob's daemon receives it through his keeper, rebuilds and checks it, and approves.
      await proposal(bob, created.id);
      const review = await bob.client.call("proposals.review", { vault, id: created.id });
      expect(review.checks).toMatchObject({ rebuilt: true, configCurrent: true, notExpired: true });
      await bob.client.call("proposals.approve", { vault, id: created.id });
      await alice.client.call("proposals.approve", { vault, id: created.id });
      await status(alice, created.id, "ready");

      const executed = await alice.client.call("proposals.execute", { vault, id: created.id });
      expect(executed.status).toBe("executed");
      await status(carol, created.id, "executed");
      expect(
        await until(
          async () => ((await balance(carol)) === minted - 100n ? true : undefined),
          "Carol's balance",
        ),
      ).toBe(true);
    });

    it("shows a transfer into the vault as received, and refuses a token deployment that is not its own", async () => {
      const bobAccount = (await bob.client.call("status", {})).profile?.account as Hex32;
      // Bob is named in aztec-wallet's aliases, which also makes his notes to the vault findable.
      const aztecWallet = await mkdtemp(join(tmpdir(), "ome-aztec-wallet-"));
      await (await openStoreAt(aztecWallet)).close();
      await alice.client.call("config.set", { aztecWalletDir: aztecWallet, aztecWalletSync: true });
      await alice.client.call("aztec.alias", { key: "accounts:bob", value: bobAccount });
      await TokenContract.at(AztecAddress.fromStringUnsafe(token), bob.daemon.wallet())
        .methods.transfer(AztecAddress.fromStringUnsafe(vault), 10n)
        .send({ from: AztecAddress.fromStringUnsafe(bobAccount), fee: bob.daemon.fee() });
      const received = await until(async () => {
        const entries = await alice.client.call("vault.activity", { vault });
        return entries.find((e) => e.kind === "received" && e.from === bobAccount);
      }, "Bob's transfer in Alice's activity");
      expect(received).toMatchObject({ kind: "received", amount: "10" });
      const [counterparty] = await alice.client.call("vault.counterparties", { vault });
      expect(counterparty).toMatchObject({ address: bobAccount, label: "bob", exposed: false });

      await expect(
        carol.client.call("tokens.add", {
          vault,
          token,
          deployment: {
            salt: `0x${"00".repeat(32)}`,
            deployer: bobAccount,
            admin: bobAccount,
            name: "Ome Test",
            symbol: "OMET",
            decimals: 18,
          },
        }),
      ).rejects.toThrow(/another address/u);
    });

    it("blocks a proposal two members reject", async () => {
      const created = await alice.client.call("proposals.create", {
        vault,
        intent: { kind: "transfer", token, to: vault, amount: "1", memo: null },
        expiresInHours: 1,
      });
      for (const m of [bob, carol]) {
        // oxlint-disable-next-line no-await-in-loop
        await proposal(m, created.id);
        // oxlint-disable-next-line no-await-in-loop
        await m.client.call("proposals.reject", { vault, id: created.id, reason: "not now" });
      }
      const blocked = await status(alice, created.id, "blocked");
      expect(blocked.rejections).toHaveLength(2);
      await expect(
        alice.client.call("proposals.execute", { vault, id: created.id }),
      ).rejects.toThrow();
    });
  });

  describe("membership", () => {
    it("a newcomer joins and Carol leaves; the newcomer's approval then counts", async () => {
      dave = await member("Dave");
      const { request } = await dave.client.call("membership.request", {});
      expect(request).toMatch(/^ome-join-v0:/u);

      // Guests joined the lobby concurrently, so look Carol's index up.
      const committee = (await alice.client.call("vault.details", { vault })).committee.members;
      const carolIndex = committee.find((m) => m.name === "Carol")?.index as number;
      const plan = await alice.client.call("membership.plan", {
        vault,
        remove: [carolIndex],
        add: [request],
        threshold: 2,
        viewingThreshold: 2,
      });
      expect(plan.failure).toBeNull();
      expect(plan.joiners.map((j) => j.name)).toEqual(["Dave"]);
      expect(plan.next.members).toEqual(["Alice", "Bob", "Dave"]);
      // The leaving member takes no part.
      await expect(
        carol.client.call("membership.begin", { plan: plan.share }),
      ).rejects.toMatchObject({
        kind: "invalid",
      });
      await Promise.all(
        [alice, bob, dave].map((m) => m.client.call("membership.begin", { plan: plan.share })),
      );

      const rotation = await until(async () => {
        const state = await alice.client.call("membership.get", { vault, id: plan.id });
        return state?.proposal ? state : undefined;
      }, "the rotation proposal");
      const id = rotation.proposal as Hex32;
      await proposal(bob, id);
      // The review runs rotate_membership itself, with the contract's own checks of the new committee.
      const review = await bob.client.call("proposals.review", { vault, id });
      expect(review.simulation).toMatchObject({ ok: true, error: null });
      await bob.client.call("proposals.approve", { vault, id });
      await alice.client.call("proposals.approve", { vault, id });
      await status(alice, id, "ready");
      await alice.client.call("proposals.execute", { vault, id });

      const done = async (m: Member) =>
        until(async () => {
          const state = (await m.client.call("membership.get", {
            vault,
            id: plan.id,
          })) as RotationState | null;
          return state?.done ? state : undefined;
        }, `${m.name}'s rotation`);
      for (const m of [alice, bob, dave]) {
        // oxlint-disable-next-line no-await-in-loop
        expect((await done(m)).error).toBeNull();
      }

      // Dave's daemon now holds the vault, its committee and its secrets, and reads its balance.
      await dave.client.call("tokens.add", { vault, token });
      expect(await until(() => balance(dave), "Dave's balance")).toBe(1000n * 10n ** 18n - 90n);

      // A payment after the rotation, approved by the newcomer.
      const bobAccount = (await bob.client.call("status", {})).profile?.account as Hex32;
      const fresh = await alice.client.call("proposals.create", {
        vault,
        intent: {
          kind: "transfer",
          token,
          to: bobAccount,
          amount: "5",
          memo: "after the rotation",
        },
        expiresInHours: 1,
      });
      await proposal(dave, fresh.id);
      await dave.client.call("proposals.approve", { vault, id: fresh.id });
      await alice.client.call("proposals.approve", { vault, id: fresh.id });
      await status(alice, fresh.id, "ready");
      expect((await alice.client.call("proposals.execute", { vault, id: fresh.id })).status).toBe(
        "executed",
      );
      const after = 1000n * 10n ** 18n - 95n;
      for (const m of [alice, dave]) {
        // oxlint-disable-next-line no-await-in-loop
        await until(
          async () => ((await balance(m)) === after ? true : undefined),
          `${m.name}'s new balance`,
        );
      }

      // Bob paid the vault while Carol was a member: she can still tell when he pays it again, so Alice is advised to
      // ask him to handshake with the vault anew.
      const [exposed] = await alice.client.call("vault.counterparties", { vault });
      expect(exposed).toMatchObject({ address: bobAccount, exposed: true });
      expect(alice.events.some((e) => e.type === "handshakeAdvised" && e.vault === vault)).toBe(
        true,
      );

      // Carol left: the new committee does not answer her keeper, so her daemon cannot see what the vault holds now
      // (at most what it knew before she left). She retires the share her keeper kept.
      await carol.client.call("tokens.add", { vault, token }).catch(() => undefined);
      const [carols] = await carol.client.call("tokens.list", { vault }).catch(() => [undefined]);
      expect(
        carols?.balance === null ||
          carols?.balance === undefined ||
          BigInt(carols.balance) !== after,
      ).toBe(true);
      expect(await carol.client.call("vault.retire", { vault })).toEqual([]);
    });
  });
});
