// Keepers that cannot reach each other except through a relay: a relay on three Docker networks and one keeper alone
// on each. The keepers run a DKG through the relay's circuits, answer threshold ECDH through them, and a keeper that
// was stopped catches up on a proposal through its relay mailbox and sync. Needs Docker; builds the ome-relay and ome-keeper images.
import { execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import type { DkgResult, Hex32, KeeperInfo, Manifest, Point } from "@ome-labs/api";
import {
  addressContext,
  buildProposal,
  committeeCommitment,
  signerSetCommitment,
  verifyAndCombine,
} from "@ome-labs/wasm";
import { publicKey } from "@ome-labs/wasm/testing";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const here = dirname(fileURLToPath(import.meta.url));
const compose = join(here, "..", "compose.yml");
const crates = join(here, "..", "..", "..", "crates");
const KEEPERS = ["keeper-a", "keeper-b", "keeper-c"] as const;
type Keeper = (typeof KEEPERS)[number];

const field = (): Hex32 => `0x00${randomBytes(31).toString("hex")}`;

function docker(args: string[], env: NodeJS.ProcessEnv = {}): string {
  return execFileSync("docker", args, {
    encoding: "utf8",
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
  });
}

let relay = "";
const composeArgs = (args: string[]) =>
  docker(["compose", "-f", compose, ...args], { RELAY: relay });

/** One RPC call on a keeper inside its container. */
function rpc<T>(keeper: Keeper, method: string, params: unknown = {}): T {
  const out = composeArgs([
    "exec",
    "-T",
    keeper,
    "ome-keeper",
    "rpc",
    "--socket",
    "/data/keeper.sock",
    "--token-file",
    "/data/rpc.token",
    method,
    JSON.stringify(params),
  ]);
  return JSON.parse(out) as T;
}

async function until<T>(check: () => T | undefined, what: string, timeoutMs = 300_000): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    try {
      const value = check();
      if (value !== undefined) return value;
    } catch {
      // Not yet: the container or method is not ready.
    }
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}

const available = (() => {
  try {
    docker(["info"]);
    return true;
  } catch {
    return false;
  }
})();

describe.skipIf(!available)("keepers on separate networks, joined by a relay", () => {
  const infos = {} as Record<Keeper, KeeperInfo>;
  let manifest: Manifest;
  let session: Hex32;

  beforeAll(async () => {
    for (const target of ["relay", "keeper"]) {
      docker([
        "build",
        "-f",
        join(crates, "relay", "deploy", "Dockerfile"),
        "--target",
        target,
        "-t",
        `ome-${target}`,
        crates,
      ]);
    }
    composeArgs(["down", "-v", "--remove-orphans"]);
    const peer = composeArgs([
      "run",
      "--rm",
      "--no-deps",
      "relay",
      "ome-relay",
      "peer-id",
      "--data-dir",
      "/data",
    ]).trim();
    // `peer-id` prints the id as a JSON string.
    relay = `/dns4/relay/tcp/4001/p2p/${JSON.parse(peer.split("\n").at(-1) ?? "") as string}`;
    composeArgs(["up", "-d", "relay"]);
    composeArgs(["up", "-d", ...KEEPERS]);
    for (const keeper of KEEPERS)
      // oxlint-disable-next-line no-await-in-loop
      infos[keeper] = await until(
        () => rpc<KeeperInfo>(keeper, "keeper.info"),
        `${keeper} to start`,
      );
  });

  afterAll(() => {
    if (available) composeArgs(["down", "-v", "--remove-orphans"]);
  });

  it("each keeper advertises a relay circuit, the only way the others can reach it", () => {
    for (const keeper of KEEPERS)
      expect(infos[keeper].addresses.some((a) => a.includes("/p2p-circuit"))).toBe(true);
  });

  it("runs a DKG through the relay's circuits", async () => {
    session = field();
    const members = KEEPERS.map((k) => ({
      identity: infos[k].identity,
      transport: infos[k].transport,
      addresses: infos[k].addresses,
    }));
    const started = Date.now();
    for (const keeper of KEEPERS) rpc(keeper, "ceremony.dkg", { session, threshold: 2, members });
    const results = await Promise.all(
      KEEPERS.map((keeper) =>
        until(() => {
          const ceremony = rpc<{ status: string; result: DkgResult | null }>(
            keeper,
            "ceremony.status",
            { session },
          );
          if (ceremony.status === "aborted") throw new Error(`${keeper}'s DKG aborted`);
          return ceremony.status === "completed" ? (ceremony.result as DkgResult) : undefined;
        }, `${keeper}'s DKG`),
      ),
    );
    console.log(
      `DKG of 3 keepers through the relay: ${((Date.now() - started) / 1000).toFixed(1)} s`,
    );
    const [first] = results as [DkgResult];
    for (const result of results) expect(result.groupKey).toEqual(first.groupKey);

    const preaddress = field();
    manifest = {
      protocolVersion: 0,
      vault: addressContext(preaddress, first.groupKey).address,
      epoch: 0,
      threshold: 2,
      viewingThreshold: 2,
      preaddress,
      groupKey: first.groupKey,
      members: KEEPERS.map((k, i) => ({
        index: i + 1,
        signerKey: publicKey(field()),
        verificationShare: first.verificationShares[i] as Point,
        identity: infos[k].identity,
        transport: infos[k].transport,
        addresses: infos[k].addresses,
      })),
    };
    for (const keeper of KEEPERS) rpc(keeper, "vault.register", { session, manifest });
  });

  it("answers threshold ECDH through the relay, and the partials combine", () => {
    const point = manifest.groupKey;
    const started = Date.now();
    const result = rpc<{ epoch: number; responses: never[] }>("keeper-a", "ecdh.batch", {
      vault: manifest.vault,
      epoch: 0,
      purpose: "decryption",
      points: [point],
    });
    console.log(`threshold read through the relay (one point): ${Date.now() - started} ms`);
    const combined = verifyAndCombine({
      epoch: 0,
      preaddress: manifest.preaddress,
      ivpk: manifest.groupKey,
      verificationShares: manifest.members.map((m) => m.verificationShare),
      threshold: 2,
      inputs: [point],
      responses: result.responses,
    });
    expect(combined.points).toHaveLength(1);
    expect(combined.invalidMembers).toEqual([]);
  });

  it("a keeper that was stopped catches up on a proposal through its relay mailbox", async () => {
    const config = {
      threshold: 2,
      signerCount: 3,
      version: 0,
      signerSetCommitment: signerSetCommitment(manifest.members.map((m) => m.signerKey)),
      viewingThreshold: 2,
      committeeCommitment: committeeCommitment(
        2,
        manifest.members.map((m) => m.verificationShare),
      ),
    };
    const built = buildProposal(
      { manifest, config },
      { chainId: field(), version: field() },
      {
        calls: [],
        authwits: [],
        extras: "0x",
        txNonce: field(),
        createdAt: Math.floor(Date.now() / 1000),
        expiresAt: Math.floor(Date.now() / 1000) + 3600,
        summary: "while keeper C was away",
        proposer: 1,
      },
    );
    composeArgs(["stop", "keeper-c"]);
    rpc("keeper-a", "payloads.put", { vault: manifest.vault, body: built.payload });
    rpc("keeper-a", "proposals.put", { vault: manifest.vault, body: built.proposal });
    composeArgs(["start", "keeper-c"]);
    const started = Date.now();
    await until(() => {
      const bodies = rpc<string[]>("keeper-c", "proposals.list", { vault: manifest.vault });
      return bodies.includes(built.proposal) ? true : undefined;
    }, "keeper C to catch up");
    console.log(`catch-up after restart: ${((Date.now() - started) / 1000).toFixed(1)} s`);
  });
});
