// Members for the end-to-end suite: each one a daemon with its own home, keeper process and PXE, driven through its
// socket like any app. The keychain is off, so the keeper's master key is the sealed file.
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createAztecNodeClient } from "@aztec-labs/aztec.js/node";
import type { DaemonEvent } from "@ome-labs/api";

import { Daemon, DaemonClient } from "../../src/index.ts";

process.env.OME_KEYCHAIN = "off";

export const NODE_URL = process.env.AZTEC_NODE_URL ?? "http://localhost:8080";
export const PASSPHRASE = "correct horse battery staple";

export async function networkAvailable(): Promise<boolean> {
  try {
    await createAztecNodeClient(NODE_URL).getNodeInfo();
    return true;
  } catch {
    return false;
  }
}

export interface Member {
  readonly name: string;
  readonly daemon: Daemon;
  readonly client: DaemonClient;
  readonly events: DaemonEvent[];
  stop(): Promise<void>;
}

/** A new member: a daemon with a fresh profile, its signer unlocked for the suite. */
export async function member(name: string): Promise<Member> {
  const home = await mkdtemp(join(tmpdir(), `ome-${name}-`));
  const daemon = new Daemon({
    profile: "default",
    home,
    settings: { nodeUrl: NODE_URL },
    keeperListen: ["/ip4/127.0.0.1/tcp/0"],
  });
  await daemon.start();
  const client = await DaemonClient.discover("default", home);
  const events: DaemonEvent[] = [];
  await client.subscribe((event) => events.push(event));
  await client.call("profile.create", { name, nodeUrl: NODE_URL, passphrase: PASSPHRASE });
  await client.call("signer.unlock", { secret: { passphrase: PASSPHRASE }, ttlSecs: 3600 });
  return {
    name,
    daemon,
    client,
    events,
    async stop() {
      client.close();
      await daemon.stop();
      await rm(home, { recursive: true, force: true });
    },
  };
}

/** Polls `check` until it returns a value. */
export async function until<T>(
  check: () => Promise<T | undefined>,
  what: string,
  timeoutMs = 600_000,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    // oxlint-disable-next-line no-await-in-loop
    const value = await check();
    if (value !== undefined) return value;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
}
