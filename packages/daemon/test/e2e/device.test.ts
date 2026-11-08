// A member moves their signing key to a phone. Three members found a vault; Alice's daemon (with a relay) pairs a
// test phone (`ome-device`, the same core as the mobile apps), and a signer swap puts the phone's key in Alice's slot,
// with no reshare. Every daemon's keeper adopts the new key; Alice's daemon then forwards proposals to the phone,
// whose approval counts toward a payment that executes, while Alice's desktop key no longer can. Revoking the phone
// ends its pairing. Runs against `aztec start --local-network` with a real `ome-relay` on loopback.
import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import type { DeviceView, FoundingState, Hex32, Point, ProposalView } from "@ome-labs/api";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { type Member, member, networkAvailable, until } from "./members.ts";

const available = await networkAvailable();
const crates = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "..", "crates");
const bin = (name: string) => join(crates, "target", "debug", name);

/** Starts `ome-relay` on loopback and returns it with its dialable address. */
async function relay(dir: string): Promise<{ process: ChildProcess; address: string }> {
  const child = spawn(
    bin("ome-relay"),
    ["run", "--data-dir", dir, "--listen", "/ip4/127.0.0.1/tcp/0"],
    {
      stdio: ["ignore", "pipe", "ignore"],
    },
  );
  if (!child.stdout) throw new Error("the relay has no stdout");
  const lines = createInterface({ input: child.stdout });
  for await (const line of lines) {
    const ready = (JSON.parse(line) as { ready?: { peerId: string; addresses: string[] } }).ready;
    if (ready) return { process: child, address: `${ready.addresses[0]}/p2p/${ready.peerId}` };
  }
  throw new Error("the relay exited before it was ready");
}

/** Runs the test phone and returns its JSON answer; a failure throws with the phone's error. */
function phone(dir: string, ...args: string[]): Promise<Record<string, unknown> & unknown[]> {
  return new Promise((resolve, reject) => {
    const child = spawn(bin("ome-device"), ["--dir", dir, ...args], {
      stdio: ["ignore", "pipe", "inherit"],
    });
    let out = "";
    child.stdout.on("data", (chunk: Buffer) => {
      out += chunk.toString("utf8");
    });
    child.on("close", (code) => {
      const value = JSON.parse(out.trim() || "{}") as Record<string, unknown> & unknown[];
      if (code === 0) resolve(value);
      else reject(new Error(String(value.error ?? `ome-device exited with ${code}`)));
    });
  });
}

const samePoint = (a: Point, b: Point) =>
  a.x.toLowerCase() === b.x.toLowerCase() && a.y.toLowerCase() === b.y.toLowerCase();

describe.skipIf(!available)("a member's signing phone", () => {
  let home: string;
  let relayProcess: ChildProcess;
  let alice: Member;
  let bob: Member;
  let carol: Member;
  let device: string;
  let phoneKey: Point;
  let paired: DeviceView;
  let vault: Hex32;

  const status = async (m: Member, id: Hex32, wanted: ProposalView["status"]) =>
    until(async () => {
      const view = await m.client.call("proposals.get", { vault, id }).catch(() => undefined);
      return view?.status === wanted ? view : undefined;
    }, `proposal ${id} to be ${wanted} at ${m.name}`);

  const slotKey = async (m: Member, name: string) =>
    (await m.client.call("vault.details", { vault })).committee.members.find((c) => c.name === name)
      ?.signerKey;

  beforeAll(async () => {
    execFileSync(
      "cargo",
      [
        "build",
        "--manifest-path",
        join(crates, "Cargo.toml"),
        "-p",
        "ome-relay",
        "-p",
        "ome-ffi",
        "--features",
        "ome-ffi/cli",
        "--bins",
      ],
      { stdio: "inherit" },
    );
    home = await mkdtemp(join(tmpdir(), "ome-phone-"));
    device = join(home, "phone");
    const started = await relay(join(home, "relay"));
    relayProcess = started.process;
    [alice, bob, carol] = await Promise.all([
      member("Alice", { relays: [started.address] }),
      member("Bob"),
      member("Carol"),
    ]);
  });

  afterAll(async () => {
    await Promise.all([alice, bob, carol].map((m) => m?.stop()));
    relayProcess?.kill();
    await rm(home, { recursive: true, force: true });
  });

  it("three daemons found a vault", async () => {
    const lobby = await alice.client.call("lobby.create", {
      name: "Treasury",
      threshold: 2,
      viewingThreshold: 2,
    });
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
    vault = states[0]?.vault as Hex32;
  });

  it("pairs a phone through the relay", async () => {
    const created = await phone(device, "create", "Alice's phone");
    phoneKey = created.signerKey as Point;
    const { offer } = await alice.client.call("devices.pair", {});
    expect(offer).toMatch(/^ome-pair-v0:/u);
    const pairing = await phone(device, "pair", offer, "--wait", "120");
    expect(pairing.member).toBe("Alice");
    const [listed] = await alice.client.call("devices.list", {});
    expect(listed?.name).toBe("Alice's phone");
    expect(listed?.vaults).toEqual([]);
    paired = listed as DeviceView;
  });

  it("moves Alice's key to the phone with a signer swap, and every keeper adopts it", async () => {
    const swap = await alice.client.call("devices.enroll", { vault, identity: paired.identity });
    await until(
      () => bob.client.call("proposals.get", { vault, id: swap.id }).catch(() => undefined),
      "the swap at Bob",
    );
    await bob.client.call("proposals.approve", { vault, id: swap.id });
    await alice.client.call("proposals.approve", { vault, id: swap.id });
    await status(alice, swap.id, "ready");
    expect((await alice.client.call("proposals.execute", { vault, id: swap.id })).status).toBe(
      "executed",
    );

    for (const m of [alice, bob, carol]) {
      // oxlint-disable-next-line no-await-in-loop
      await until(async () => {
        const key = await slotKey(m, "Alice");
        return key && samePoint(key, phoneKey) ? true : undefined;
      }, `${m.name}'s keeper to adopt the phone's key`);
    }
    const [holder] = await alice.client.call("devices.list", {});
    expect(holder?.vaults).toEqual([vault]);
  });

  it("the phone's approval counts toward a payment that executes; Alice's desktop key no longer does", async () => {
    const token = (await alice.client.call("tokens.deployTest", { vault })).address;
    await bob.client.call("tokens.add", { vault, token });
    const bobAccount = (await bob.client.call("status", {})).profile?.account as Hex32;
    const payment = await bob.client.call("proposals.create", {
      vault,
      intent: { kind: "transfer", token, to: bobAccount, amount: "25", memo: "from the phone" },
      expiresInHours: 1,
    });

    // Alice's daemon forwards it to the phone, which checks it on its own.
    const review = await until(async () => {
      await phone(device, "sync");
      const reviews = (await phone(device, "reviews")) as unknown as {
        id: string;
        signable: boolean;
      }[];
      return reviews.find((r) => r.id === payment.id);
    }, "the payment to reach the phone");
    expect(review.signable).toBe(true);
    await expect(alice.client.call("proposals.approve", { vault, id: payment.id })).rejects.toThrow(
      /paired device/u,
    );

    const approved = await phone(device, "approve", payment.id);
    expect(approved.status).toBe("Approved");
    await until(async () => {
      const view = await bob.client.call("proposals.get", { vault, id: payment.id });
      return view.approvals.some((v) => v.slot === 0) ? true : undefined;
    }, "the phone's approval to reach Bob");
    await bob.client.call("proposals.approve", { vault, id: payment.id });
    await status(bob, payment.id, "ready");
    expect((await bob.client.call("proposals.execute", { vault, id: payment.id })).status).toBe(
      "executed",
    );
    await until(async () => {
      await phone(device, "sync");
      const reviews = (await phone(device, "reviews")) as unknown as {
        id: string;
        outcome: string | null;
      }[];
      return reviews.find((r) => r.id === payment.id)?.outcome === "accepted" ? true : undefined;
    }, "the keeper's answer to the phone's vote");
  });

  it("revoking the phone ends its pairing", async () => {
    await alice.client.call("devices.revoke", { identity: paired.identity });
    expect(await alice.client.call("devices.list", {})).toEqual([]);
    const synced = await until(async () => {
      const result = await phone(device, "sync").catch(() => undefined);
      return result?.unpaired ? result : undefined;
    }, "the phone to learn it was unpaired");
    expect(synced.unpaired).toBe(true);
  });
});
