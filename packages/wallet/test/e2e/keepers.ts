// Real `ome-keeper` processes for the end-to-end suite. The wallet only connects to keepers; launching them is the
// daemon's job, so this helper does it for the tests: master key and RPC token on stdin, never on the
// command line or in the environment.
import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import type {
  Ceremony,
  DkgResult,
  Hex32,
  KeeperContact,
  Manifest,
  NewMember,
  Point,
  ReshareResult,
} from "@ome-labs/api";

import { KeeperClient } from "../../src/index.ts";

const CRATES = fileURLToPath(new URL("../../../../crates/", import.meta.url));
const BINARY = process.env.OME_KEEPER_BIN ?? join(CRATES, "target", "debug", "ome-keeper");

/** A running keeper with an authenticated client. */
export interface Keeper {
  readonly client: KeeperClient;
  readonly contact: KeeperContact;
  stop(): Promise<void>;
}

function binary(): string {
  if (!existsSync(BINARY)) {
    execFileSync(
      "cargo",
      ["build", "--manifest-path", join(CRATES, "Cargo.toml"), "-p", "ome-keeper"],
      {
        stdio: "inherit",
      },
    );
  }
  return BINARY;
}

/** Starts `ome-keeper run` in a fresh data directory and connects to it. */
export async function launchKeeper(): Promise<Keeper> {
  const dataDir = await mkdtemp(join(tmpdir(), "ome-keeper-"));
  const token = randomBytes(32).toString("hex");
  const child: ChildProcess = spawn(binary(), ["run", "--data-dir", dataDir, "--secrets-stdin"], {
    stdio: ["pipe", "pipe", "ignore"],
  });
  const { stdin, stdout } = child;
  if (!stdin || !stdout) throw new Error("keeper started without pipes");
  stdin.end(`${JSON.stringify({ masterKey: randomBytes(32).toString("hex"), rpcToken: token })}\n`);
  const ready = await new Promise<{
    identity: Hex32;
    transport: Hex32;
    addresses: string[];
    rpcSocket: string;
  }>((resolve, reject) => {
    createInterface({ input: stdout }).once("line", (line) => {
      try {
        resolve((JSON.parse(line) as { ready: never }).ready);
      } catch (error) {
        reject(error);
      }
    });
    child.once("exit", (code) =>
      reject(new Error(`ome-keeper exited with ${code} before it was ready`)),
    );
  });
  const client = await KeeperClient.connect(ready.rpcSocket, token);
  return {
    client,
    contact: { identity: ready.identity, transport: ready.transport, addresses: ready.addresses },
    async stop() {
      client.close();
      if (child.exitCode === null) {
        const exited = new Promise((resolve) => child.once("exit", resolve));
        child.kill("SIGTERM");
        await exited;
      }
      await rm(dataDir, { recursive: true, force: true });
    },
  };
}

export const session = (): Hex32 => `0x${randomBytes(32).toString("hex")}`;

/** Polls a ceremony until it completes; throws if it aborts. */
async function finished(keeper: Keeper, id: Hex32): Promise<Ceremony> {
  for (let i = 0; i < 1200; i++) {
    // oxlint-disable-next-line no-await-in-loop
    const ceremony = await keeper.client.call("ceremony.status", { session: id });
    if (ceremony.status === "aborted")
      throw new Error(`ceremony aborted: ${ceremony.failure?.reason}`);
    if (ceremony.status === "completed") return ceremony;
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`ceremony ${id} did not finish`);
}

/** Runs the founding DKG among `keepers`, checks they agree, and returns the result. */
export async function runDkg(keepers: readonly Keeper[], threshold: number) {
  const id = session();
  const members = keepers.map((k) => k.contact);
  await Promise.all(
    keepers.map((k) => k.client.call("ceremony.dkg", { session: id, threshold, members })),
  );
  const results = (await Promise.all(keepers.map((k) => finished(k, id)))).map(
    (c) => c.result as DkgResult,
  );
  if (new Set(results.map((r) => r.transcript)).size !== 1)
    throw new Error("keepers disagree on the DKG transcript");
  return { session: id, result: results[0] as DkgResult };
}

/**
 * Reshares `vault` from `dealers` (staying members' keepers) to `next`, the new committee in member order with their
 * signer keys. A joining keeper is given the current manifest.
 */
export async function runReshare(options: {
  vault: Hex32;
  current: Manifest;
  dealers: readonly { keeper: Keeper; index: number }[];
  next: readonly { keeper: Keeper; signerKey: Point }[];
  threshold: number;
  viewingThreshold: number;
}) {
  const id = session();
  const newMembers: NewMember[] = options.next.map(({ keeper, signerKey }) => ({
    ...keeper.contact,
    signerKey,
  }));
  const request = {
    session: id,
    vault: options.vault,
    current: null,
    dealers: options.dealers.map((d) => d.index),
    newThreshold: options.viewingThreshold,
    newSpendingThreshold: options.threshold,
    newMembers,
  };
  const dealers = new Set(options.dealers.map((d) => d.keeper));
  const participants = [...new Set([...dealers, ...options.next.map((n) => n.keeper)])];
  await Promise.all(
    participants.map((k) =>
      k.client.call(
        "ceremony.reshare",
        dealers.has(k) ? request : { ...request, current: options.current },
      ),
    ),
  );
  const results = (await Promise.all(options.next.map(({ keeper }) => finished(keeper, id)))).map(
    (c) => c.result as ReshareResult,
  );
  const result = results[0];
  if (!result?.manifest || !result.transcript) throw new Error("reshare produced no manifest");
  return { session: id, result };
}
