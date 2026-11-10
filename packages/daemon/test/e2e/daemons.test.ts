// Three members found a 2-of-3 vault through their daemons, pay from it with two approvals, call a contract only the
// proposer registered, see a rejection block a proposal, and then rotate: Carol leaves and a newcomer joins with a join request, receiving the vault's protocol
// secrets in the reshare. Everything goes through the daemons' sockets, as an app would; each daemon runs its own
// keeper process and PXE against `aztec start --local-network`.
import { existsSync } from "node:fs";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { openStoreAt } from "@aztec-labs/kv-store/lmdb-v2";
import { PrivateVotingContract } from "@aztec-labs/noir-contracts.js/PrivateVoting";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import { contractArtifactToBuffer } from "@aztec-labs/stdlib/abi";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { FoundingState, Hex32, ProposalView, RotationState } from "@ome-labs/api";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { type Member, PASSPHRASE, member, networkAvailable, until } from "./members.ts";

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
    return entry?.private.state === "known" ? BigInt(entry.private.amount) : undefined;
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
        members: 3,
      });
      expect(lobby.invite).toMatch(/^ome-invite-v0:/u);
      expect(lobby.expected).toBe(3);
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
      // Every member knows every member's own account, from the lobby.
      const accounts = await Promise.all(
        [alice, bob, carol].map(async (m) => (await m.client.call("status", {})).profile?.account),
      );
      for (const m of [alice, bob, carol]) {
        // oxlint-disable-next-line no-await-in-loop
        const details = await m.client.call("vault.details", { vault });
        // Slots follow the order members joined the lobby.
        expect(details.committee.members.map((c) => c.account).toSorted()).toEqual(
          accounts.toSorted(),
        );
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
      // The recipient is a member's own account: named as the member.
      expect(review.recipient).toMatchObject({ label: "Bob", source: "member" });
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
        return entries.items.find((e) => e.kind === "received" && e.from === bobAccount);
      }, "Bob's transfer in Alice's activity");
      expect(received).toMatchObject({ kind: "received", amount: "10" });
      const {
        items: [counterparty],
      } = await alice.client.call("vault.counterparties", { vault });
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

    it("finds a payment from a sender nobody registered, through its non-interactive handshake", async () => {
      // Carol is no alias on any member's machine. Her wallet pays with Aztec's default strategy, a non-interactive
      // handshake, which the vault's sync finds in the handshake registry through threshold ECDH.
      const aliceAccount = AztecAddress.fromStringUnsafe(
        (await alice.client.call("status", {})).profile?.account as Hex32,
      );
      const carolAccount = (await carol.client.call("status", {})).profile?.account as Hex32;
      await TokenContract.at(AztecAddress.fromStringUnsafe(token), alice.daemon.wallet())
        .methods.mint_to_private(AztecAddress.fromStringUnsafe(carolAccount), 50n)
        .send({ from: aliceAccount, fee: alice.daemon.fee() });
      await TokenContract.at(AztecAddress.fromStringUnsafe(token), carol.daemon.wallet())
        .methods.transfer(AztecAddress.fromStringUnsafe(vault), 7n)
        .send({ from: AztecAddress.fromStringUnsafe(carolAccount), fee: carol.daemon.fee() });
      const received = await until(
        async () => {
          const entries = await bob.client.call("vault.activity", { vault });
          return entries.items.find((e) => e.kind === "received" && e.from === carolAccount);
        },
        "Carol's payment in Bob's activity",
        240_000,
        2000,
      );
      expect(received).toMatchObject({ kind: "received", amount: "7" });
    });

    it("calls a contract only the proposer has: the others fetch its code, review it decoded, and execute it", async () => {
      const aliceAccount = AztecAddress.fromStringUnsafe(
        (await alice.client.call("status", {})).profile?.account as Hex32,
      );
      const { contract } = await PrivateVotingContract.deploy(
        alice.daemon.wallet(),
        aliceAccount,
      ).send({
        from: aliceAccount,
        fee: alice.daemon.fee(),
      });
      // Votes count only in a started election; Alice is the contract's admin.
      await contract.methods
        .start_vote({ id: 1n })
        .send({ from: aliceAccount, fee: alice.daemon.fee() });
      const address = contract.address.toString() as Hex32;
      const artifact = contractArtifactToBuffer(PrivateVotingContract.artifact).toString("utf8");
      await alice.client.call("contracts.register", { address, artifact });
      const functions = await alice.client.call("contracts.functions", { contract: address });
      expect(functions.find((f) => f.name === "cast_vote")?.params).toEqual([
        {
          name: "election_id",
          type: {
            kind: "struct",
            path: "PrivateVoting::ElectionId",
            fields: [{ name: "id", type: { kind: "field" } }],
          },
        },
        { name: "candidate", type: { kind: "field" } },
      ]);
      // A bad argument fails the dry run itself, naming the parameter.
      await expect(
        alice.client.call("proposals.preview", {
          vault,
          intent: {
            kind: "calls",
            calls: [{ to: address, fn: "cast_vote", args: [{ id: "1" }, "1.5"] }],
            memo: null,
          },
          expiresInHours: 1,
        }),
      ).rejects.toThrow(/cast_vote\.candidate/u);

      const created = await alice.client.call("proposals.create", {
        vault,
        intent: {
          kind: "calls",
          calls: [{ to: address, fn: "cast_vote", args: [{ id: "1" }, "7"] }],
          memo: "vote 7",
        },
        expiresInHours: 1,
      });
      // Bob never registered the contract: his keeper fetches it from Alice's, his daemon checks it against the class.
      await proposal(bob, created.id);
      const review = await until(
        async () => {
          const r = await bob.client.call("proposals.review", { vault, id: created.id });
          return r.proposal.calls.find((c) => c.fn === "cast_vote")?.decoded ? r : undefined;
        },
        "Bob's decoded review",
        180_000,
        2000,
      );
      const vote = review.proposal.calls.find((c) => c.fn === "cast_vote");
      expect(vote?.args.map((a) => a.name)).toEqual(["election_id", "candidate"]);
      expect(review.simulation).toMatchObject({ ok: true, error: null });
      expect(bob.events.some((e) => e.type === "contractAvailable")).toBe(true);

      await bob.client.call("proposals.approve", { vault, id: created.id });
      await alice.client.call("proposals.approve", { vault, id: created.id });
      await status(bob, created.id, "ready");
      // Bob proves it with the code he fetched.
      expect((await bob.client.call("proposals.execute", { vault, id: created.id })).status).toBe(
        "executed",
      );
    });

    it("shows the vault's balance before and after, names a shortfall, and reviews a threshold change", async () => {
      const bobAccount = (await bob.client.call("status", {})).profile?.account as Hex32;
      const held = await until(() => balance(alice), "Alice's balance");
      const transfer = (amount: bigint) =>
        alice.client.call("proposals.preview", {
          vault,
          intent: {
            kind: "transfer",
            token,
            to: bobAccount,
            amount: amount.toString(),
            memo: null,
          },
          expiresInHours: 1,
        });

      const paying = await transfer(25n);
      expect(paying.ok).toBe(true);
      expect(paying.effects?.complete).toBe(true);
      expect(paying.effects?.balances).toEqual([
        expect.objectContaining({
          before: held.toString(),
          after: (held - 25n).toString(),
          visibility: "private",
        }),
      ]);
      const tooMuch = await transfer(held + 1n);
      expect(tooMuch.ok).toBe(false);
      expect(tooMuch.error?.failure).toMatchObject({
        kind: "insufficientBalance",
        contract: token,
      });

      const raise = await alice.client.call("proposals.create", {
        vault,
        intent: { kind: "setThreshold", threshold: 3, memo: null },
        expiresInHours: 1,
      });
      await proposal(bob, raise.id);
      const review = await bob.client.call("proposals.review", { vault, id: raise.id });
      expect(review.change).toMatchObject({ kind: "threshold", before: 2, after: 3, members: 3 });
      expect(review.change?.kind === "threshold" && review.change.warnings).toContain(
        "no-spare-signer",
      );
      for (const m of [alice, bob]) {
        // oxlint-disable-next-line no-await-in-loop
        await m.client.call("proposals.approve", { vault, id: raise.id });
      }
      await status(alice, raise.id, "ready");
      expect((await alice.client.call("proposals.execute", { vault, id: raise.id })).status).toBe(
        "executed",
      );

      // Back to 2 of 3 for the suite's later tests; it now takes all three.
      const back = await alice.client.call("proposals.create", {
        vault,
        intent: { kind: "setThreshold", threshold: 2, memo: null },
        expiresInHours: 1,
      });
      for (const m of [alice, bob, carol]) {
        // oxlint-disable-next-line no-await-in-loop
        await proposal(m, back.id);
        // oxlint-disable-next-line no-await-in-loop
        await m.client.call("proposals.approve", { vault, id: back.id });
      }
      await status(alice, back.id, "ready");
      expect((await alice.client.call("proposals.execute", { vault, id: back.id })).status).toBe(
        "executed",
      );
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
      expect(blocked.votes.filter((v) => !v.approve)).toHaveLength(2);
      await expect(
        alice.client.call("proposals.execute", { vault, id: created.id }),
      ).rejects.toThrow();
    });
  });

  describe("lifecycle", () => {
    it("shows who executes a proposal while it runs, refuses a second executor, and names the executor", async () => {
      const fees = await alice.client.call("fees.status", {});
      expect(fees).toMatchObject({ kind: "sponsored", healthy: true });
      const bobAccount = (await bob.client.call("status", {})).profile?.account as Hex32;
      const intent = { kind: "transfer" as const, token, to: bobAccount, amount: "1", memo: null };
      const preview = await alice.client.call("proposals.preview", {
        vault,
        intent,
        expiresInHours: 1,
      });
      expect(BigInt(preview.fee?.fee ?? "0")).toBeGreaterThan(0n);
      expect(BigInt(preview.fee?.maxFee ?? "0")).toBeGreaterThan(BigInt(preview.fee?.fee ?? "0"));

      const created = await alice.client.call("proposals.create", {
        vault,
        intent,
        expiresInHours: 1,
      });
      expect(created.createdAt).toBeGreaterThan(0);
      expect(created.createdAt).toBeLessThan(created.expiresAt);
      await proposal(bob, created.id);
      for (const m of [alice, bob]) {
        // oxlint-disable-next-line no-await-in-loop
        await m.client.call("proposals.approve", { vault, id: created.id });
      }
      const ready = await status(bob, created.id, "ready");
      expect(ready.createdAt).toBe(created.createdAt);
      expect(ready.votes.map((v) => v.at).every((at) => at > 0)).toBe(true);

      // Alice proves for a while; Bob's daemon sees her claim and won't prove the same transaction.
      const executing = alice.client.call("proposals.execute", { vault, id: created.id });
      const seen = await until(async () => {
        const view = await bob.client.call("proposals.get", { vault, id: created.id });
        return view.executing ?? undefined;
      }, "Alice's claim at Bob");
      expect(seen).toMatchObject({ member: { name: "Alice" }, mine: false });
      await expect(
        bob.client.call("proposals.execute", { vault, id: created.id }),
      ).rejects.toMatchObject({ kind: "conflict" });
      expect((await executing).status).toBe("executed");

      for (const m of [alice, bob, carol]) {
        // oxlint-disable-next-line no-await-in-loop
        const view = await status(m, created.id, "executed");
        expect(view.execution?.executor?.name).toBe("Alice");
        expect(view.execution?.block).toBeGreaterThan(0);
        expect(view.executing).toBeNull();
      }
      const events = bob.events.filter((e) => "proposal" in e && e.proposal === created.id);
      expect(events.some((e) => e.type === "proposalExecuting")).toBe(true);
      expect(events.some((e) => e.type === "proposalExecuted" && e.by?.name === "Alice")).toBe(
        true,
      );
    });
  });

  describe("members and profiles", () => {
    it("renames the vault and a member for everyone", async () => {
      await alice.client.call("vault.rename", { vault, name: "Operations" });
      await until(async () => {
        const [summary] = await bob.client.call("vault.list", {});
        return summary?.name === "Operations" ? true : undefined;
      }, "the new name at Bob");
      expect(
        bob.events.some(
          (e) => e.type === "vaultRenamed" && e.name === "Operations" && e.by.name === "Alice",
        ),
      ).toBe(true);

      const renamed = await bob.client.call("profile.rename", { name: "Robert" });
      expect(renamed.name).toBe("Robert");
      await until(async () => {
        const details = await alice.client.call("vault.details", { vault });
        return details.committee.members.some((m) => m.name === "Robert") ? true : undefined;
      }, "Bob's new name at Alice");
    });

    it("changes the passphrase: the old one stops working, the new one opens the key and the keeper", async () => {
      const before = (await alice.client.call("status", {})).profile?.signerKey;
      await expect(
        alice.client.call("signer.changePassphrase", {
          current: { passphrase: "not the passphrase" },
          next: "a much newer passphrase",
        }),
      ).rejects.toThrow();
      await alice.client.call("signer.changePassphrase", {
        current: { passphrase: PASSPHRASE },
        next: "a much newer passphrase",
      });
      expect(alice.events.some((e) => e.type === "sealingKeyChanged")).toBe(true);
      await expect(
        alice.client.call("signer.unlock", { secret: { passphrase: PASSPHRASE }, ttlSecs: 60 }),
      ).rejects.toThrow();
      await alice.client.call("signer.unlock", {
        secret: { passphrase: "a much newer passphrase" },
        ttlSecs: 3600,
      });
      expect((await alice.client.call("status", {})).profile?.signerKey).toEqual(before);
      // Back to the suite's passphrase for the tests that follow.
      await alice.client.call("signer.changePassphrase", {
        current: { passphrase: "a much newer passphrase" },
        next: PASSPHRASE,
      });
      await alice.client.call("signer.unlock", {
        secret: { passphrase: PASSPHRASE },
        ttlSecs: 3600,
      });
    });

    it("refuses to delete a profile that is in a vault, and deletes one that is not", async () => {
      await expect(alice.client.call("profile.delete", { confirm: "default" })).rejects.toThrow(
        /member of Operations/u,
      );
      const erin = await member("Erin");
      await erin.client.call("profile.delete", { confirm: "default" });
      await until(
        async () => (existsSync(erin.profileDir) ? undefined : true),
        "Erin's profile to be gone",
      );
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
      // Bob renamed himself earlier: the plan carries his current name to the newcomer.
      expect(plan.next.members).toEqual(["Alice", "Robert", "Dave"]);
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

      // Dave's daemon now holds the vault, its committee and its secrets, and reads the balance Alice reads.
      await dave.client.call("tokens.add", { vault, token });
      expect(await until(() => balance(dave), "Dave's balance")).toBe(await balance(alice));

      // A payment after the rotation, approved by the newcomer.
      const before = (await balance(alice)) ?? 0n;
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
      const after = before - 5n;
      for (const m of [alice, dave]) {
        // oxlint-disable-next-line no-await-in-loop
        await until(
          async () => ((await balance(m)) === after ? true : undefined),
          `${m.name}'s new balance`,
        );
      }

      // Bob paid the vault while Carol was a member: she can still tell when he pays it again, so Alice is advised to
      // ask him to handshake with the vault anew.
      const { items } = await alice.client.call("vault.counterparties", { vault });
      expect(items.find((c) => c.address === bobAccount)).toMatchObject({ exposed: true });
      expect(alice.events.some((e) => e.type === "handshakeAdvised" && e.vault === vault)).toBe(
        true,
      );

      // Carol left: the new committee does not answer her keeper, so her daemon cannot see what the vault holds now
      // (at most what it knew before she left). She retires the share her keeper kept.
      await carol.client.call("tokens.add", { vault, token }).catch(() => undefined);
      const [carols] = await carol.client.call("tokens.list", { vault }).catch(() => [undefined]);
      expect(carols?.private.state !== "known" || BigInt(carols.private.amount) !== after).toBe(
        true,
      );
      expect(await carol.client.call("vault.retire", { vault })).toEqual([]);
    });
  });

  describe("reading", () => {
    it("pages proposals, filters what needs a vote, and counts it for the vault", async () => {
      const first = await alice.client.call("proposals.list", { vault, limit: 2 });
      expect(first.items).toHaveLength(2);
      expect(first.next).not.toBeNull();
      const rest = await alice.client.call("proposals.list", {
        vault,
        limit: 500,
        before: first.next ?? undefined,
      });
      const all = await alice.client.call("proposals.list", { vault, limit: 500 });
      expect([...first.items, ...rest.items].map((p) => p.id)).toEqual(all.items.map((p) => p.id));
      expect(all.items.map((p) => p.createdAt)).toEqual(
        all.items.map((p) => p.createdAt).toSorted((a, b) => b - a),
      );
      const needs = await alice.client.call("proposals.list", { vault, filter: "needsMe" });
      const summary = (await alice.client.call("status", {})).vaults.find(
        (v) => v.address === vault,
      );
      expect(summary?.counts.needsMe).toBe(needs.items.length);
      const request = await alice.client.call("vault.request", { vault, token, amount: "25" });
      expect(request.uri).toMatch(/^ome-pay:0x[0-9a-f]{64}\?network=aztec/u);
      expect((await alice.client.call("vault.receiving", { vault })).handshakes).toBe(true);
    });

    it("says how many keepers answered when too few can read, and still shows the public balance", async () => {
      // Two of the three members' keepers go offline; a new note to the vault then needs t_v = 2 of them to read.
      await Promise.all([bob, dave].map((m) => m.daemon.keeper.stop()));
      const aliceAccount = AztecAddress.fromStringUnsafe(
        (await alice.client.call("status", {})).profile?.account as Hex32,
      );
      await TokenContract.at(AztecAddress.fromStringUnsafe(token), alice.daemon.wallet())
        .methods.mint_to_private(AztecAddress.fromStringUnsafe(vault), 3n)
        .send({ from: aliceAccount, fee: alice.daemon.fee() });
      const [read] = (await alice.client.call("tokens.list", { vault })) ?? [];
      expect(read?.private).toEqual({ state: "unreadable", answered: 1, needed: 2 });
      expect(read?.public).toEqual({ state: "known", amount: "0" });
    });
  });
});
