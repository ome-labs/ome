// A member whose keeper runs on their own server: the keeper serves its RPC over TLS (`--rpc-listen`) and Bob's
// daemon uses it instead of its own, pinning its certificate's fingerprint. Alice (local keeper) and Bob (remote
// keeper) found a vault, pay from it, and Bob reads the vault through the remote keeper. Runs against
// `aztec start --local-network`.
import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import type { FoundingState, Hex32, ProposalView } from "@ome-labs/api";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { type Member, member, networkAvailable, until } from "./members.ts";

const available = await networkAvailable();
const crates = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "..", "crates");

interface Ready {
  identity: string;
  rpcAddress: string;
  rpcFingerprint: string;
}

/** Runs a keeper as on a server: a passphrase, RPC over TLS on loopback. */
async function serverKeeper(dir: string): Promise<{ process: ChildProcess; ready: Ready }> {
  const child = spawn(
    join(crates, "target", "debug", "ome-keeper"),
    ["run", "--data-dir", dir, "--rpc-listen", "127.0.0.1:0", "--listen", "/ip4/127.0.0.1/tcp/0"],
    {
      stdio: ["ignore", "pipe", "ignore"],
      env: { ...process.env, OME_KEEPER_PASSPHRASE: "server keeper passphrase" },
    },
  );
  if (!child.stdout) throw new Error("the keeper has no stdout");
  for await (const line of createInterface({ input: child.stdout })) {
    const ready = (JSON.parse(line) as { ready?: Ready }).ready;
    if (ready) return { process: child, ready };
  }
  throw new Error("the keeper exited before it was ready");
}

describe.skipIf(!available)("a member's keeper on their own server", () => {
  let home: string;
  let keeper: ChildProcess;
  let ready: Ready;
  let token: string;
  let alice: Member;
  let bob: Member;
  let vault: Hex32;

  const status = async (m: Member, id: Hex32, wanted: ProposalView["status"]) =>
    until(async () => {
      const view = await m.client.call("proposals.get", { vault, id }).catch(() => undefined);
      return view?.status === wanted ? view : undefined;
    }, `proposal ${id} to be ${wanted} at ${m.name}`);

  beforeAll(async () => {
    execFileSync(
      "cargo",
      ["build", "--manifest-path", join(crates, "Cargo.toml"), "-p", "ome-keeper"],
      {
        stdio: "inherit",
      },
    );
    home = await mkdtemp(join(tmpdir(), "ome-server-"));
    ({ process: keeper, ready } = await serverKeeper(join(home, "keeper")));
    token = (await readFile(join(home, "keeper", "rpc.token"), "utf8")).trim();
    [alice, bob] = await Promise.all([member("Alice"), member("Bob")]);
  });

  afterAll(async () => {
    await Promise.all([alice, bob].map((m) => m?.stop()));
    keeper?.kill();
    await rm(home, { recursive: true, force: true });
  });

  it("Bob's daemon uses the server's keeper, pinned by its fingerprint", async () => {
    await expect(
      bob.client.call("keeper.connect", {
        remote: { address: ready.rpcAddress, fingerprint: "00".repeat(32), token },
      }),
    ).rejects.toThrow(/another certificate/u);
    const state = await bob.client.call("keeper.connect", {
      remote: { address: ready.rpcAddress, fingerprint: ready.rpcFingerprint, token },
    });
    expect(state).toMatchObject({
      status: "running",
      remote: ready.rpcAddress,
      identity: ready.identity,
    });
    expect((await bob.client.call("config.get", {})).remoteKeeper).toEqual({
      address: ready.rpcAddress,
      fingerprint: ready.rpcFingerprint,
    });
  });

  it("Alice and Bob found a vault and pay from it; Bob reads it through the remote keeper", async () => {
    const lobby = await alice.client.call("lobby.create", {
      name: "Studio",
      threshold: 2,
      viewingThreshold: 2,
    });
    await bob.client.call("lobby.join", { invite: lobby.invite ?? "" });
    await until(async () => {
      const view = await alice.client.call("lobby.get", { lobby: lobby.lobby });
      return view.members.length === 2 ? view : undefined;
    }, "Bob to join");
    await alice.client.call("lobby.start", {
      lobby: lobby.lobby,
      threshold: 2,
      viewingThreshold: 2,
    });
    const states = await Promise.all(
      [alice, bob].map(async (m) => {
        await m.client.call("founding.begin", { lobby: lobby.lobby });
        return until(async () => {
          const state = await m.client.call("founding.get", { lobby: lobby.lobby });
          return state?.done ? state : undefined;
        }, `${m.name}'s founding`);
      }),
    );
    for (const state of states as FoundingState[]) expect(state.error).toBeNull();
    vault = states[0]?.vault as Hex32;

    const asset = (await alice.client.call("tokens.deployTest", { vault })).address;
    await bob.client.call("tokens.add", { vault, token: asset });
    const balance = async () => {
      const [entry] = await bob.client.call("tokens.list", { vault });
      return entry?.balance ? BigInt(entry.balance) : undefined;
    };
    expect(await until(balance, "Bob's balance")).toBe(1000n * 10n ** 18n);

    const aliceAccount = (await alice.client.call("status", {})).profile?.account as Hex32;
    const payment = await bob.client.call("proposals.create", {
      vault,
      intent: { kind: "transfer", token: asset, to: aliceAccount, amount: "7", memo: null },
      expiresInHours: 1,
    });
    await until(
      () => alice.client.call("proposals.get", { vault, id: payment.id }).catch(() => undefined),
      "the payment at Alice",
    );
    await bob.client.call("proposals.approve", { vault, id: payment.id });
    await alice.client.call("proposals.approve", { vault, id: payment.id });
    await status(bob, payment.id, "ready");
    expect((await bob.client.call("proposals.execute", { vault, id: payment.id })).status).toBe(
      "executed",
    );
    await until(
      async () => ((await balance()) === 1000n * 10n ** 18n - 7n ? true : undefined),
      "Bob's new balance",
    );
  });
});
