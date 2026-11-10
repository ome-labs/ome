// Members for the end-to-end suite: each one a daemon with its own home, keeper process and PXE, driven through its
// socket like any app. The keychain is off, so the keeper's master key is the sealed file.
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createAztecNodeClient } from "@aztec-labs/aztec.js/node";
import type { DaemonEvent, Hex32, Settings } from "@ome-labs/api";

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
  /** The profile's directory, which `profile.delete` removes. */
  readonly profileDir: string;
  stop(): Promise<void>;
}

/** A new member: a daemon with a fresh profile, its signer unlocked for the suite. */
export async function member(name: string, settings: Partial<Settings> = {}): Promise<Member> {
  const home = await mkdtemp(join(tmpdir(), `ome-${name}-`));
  const daemon = new Daemon({
    profile: "default",
    home,
    // No release checks from tests.
    settings: { nodeUrl: NODE_URL, updateCheck: false, ...settings },
    keeperListen: ["/ip4/127.0.0.1/tcp/0"],
    // A deleted profile's daemon stops; it must not signal the test process.
    exit: () => undefined,
    // The app page, on a free port: apps connect through it in the app connection test.
    appsPort: 0,
  });
  await daemon.start();
  const client = await DaemonClient.discover("default", home);
  const events: DaemonEvent[] = [];
  await client.subscribe((event) => events.push(event));
  await client.call("profile.create", {
    name,
    nodeUrl: settings.nodeUrl ?? NODE_URL,
    passphrase: PASSPHRASE,
  });
  await client.call("signer.unlock", { secret: { passphrase: PASSPHRASE }, ttlSecs: 3600 });
  return {
    name,
    daemon,
    client,
    events,
    profileDir: join(home, "profiles", "default"),
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
  intervalMs = 500,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    // oxlint-disable-next-line no-await-in-loop
    const value = await check();
    if (value !== undefined) return value;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
}

/** The members found a vault through a lobby the first one hosts; returns its address. */
export async function found(
  members: readonly Member[],
  name: string,
  threshold: number,
  viewingThreshold: number,
): Promise<Hex32> {
  const [host, ...guests] = members;
  if (!host) throw new Error("a vault needs members");
  const lobby = await host.client.call("lobby.create", {
    name,
    threshold,
    viewingThreshold,
    members: members.length,
  });
  for (const guest of guests)
    // oxlint-disable-next-line no-await-in-loop
    await guest.client.call("lobby.join", { invite: lobby.invite ?? "" });
  await until(async () => {
    const view = await host.client.call("lobby.get", { lobby: lobby.lobby });
    return view.members.length === members.length ? view : undefined;
  }, "every guest to join");
  await host.client.call("lobby.start", { lobby: lobby.lobby, threshold, viewingThreshold });
  const states = await Promise.all(
    members.map(async (m) => {
      await m.client.call("founding.begin", { lobby: lobby.lobby });
      return until(async () => {
        const state = await m.client.call("founding.get", { lobby: lobby.lobby });
        return state?.done ? state : undefined;
      }, `${m.name}'s founding`);
    }),
  );
  const failed = states.find((s) => s.error);
  if (failed) throw new Error(`founding failed: ${JSON.stringify(failed.error)}`);
  return states[0]?.vault as Hex32;
}
