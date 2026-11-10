// The daemon's socket: sessions must authenticate, unknown methods and handler failures come back with the API's
// error kinds, codes and details, numbered events reach subscribed sessions and replay to ones that reconnect, and the
// socket is owner-only.
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type DaemonEvent, ERROR_CODES, type EventEnvelope } from "@ome-labs/api";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { type DaemonCallError, DaemonClient } from "../client.ts";
import { DaemonError } from "../errors.ts";
import { DaemonServer } from "./server.ts";

const TOKEN = "launch-token";

const kindOf = (error: unknown) => (error as DaemonCallError).kind;

describe("DaemonServer", () => {
  let dir: string;
  let socket: string;
  let server: DaemonServer;

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), "omed-"));
    socket = join(dir, "daemon.sock");
    server = await DaemonServer.listen(
      socket,
      TOKEN,
      new Set(["daemon.info", "status", "signer.lock", "signer.status"]),
      async (method) => {
        if (method === "status") throw new DaemonError("not-ready", "starting");
        if (method === "signer.status")
          throw new Error("note sync failed", {
            cause: DaemonError.detailed(
              { kind: "unreadable", answered: 1, needed: 2 },
              "only 1 of 2 keepers answered",
            ),
          });
        if (method === "signer.lock")
          throw Object.assign(new Error("wasm says no"), { kind: "auth" });
        return { method };
      },
    );
  });

  afterEach(async () => {
    await server.close();
    await rm(dir, { recursive: true, force: true });
  });

  it("refuses a session without the token and serves one with it", async () => {
    await expect(DaemonClient.connect(socket, "wrong")).rejects.toMatchObject({
      kind: "unauthorized",
    });
    const client = await DaemonClient.connect(socket, TOKEN);
    expect(await client.call("daemon.info", {})).toEqual({ method: "daemon.info" });
    expect(((await stat(socket)).mode & 0o777).toString(8)).toBe("600");
    client.close();
  });

  it("maps failures to the API's error kinds", async () => {
    const client = await DaemonClient.connect(socket, TOKEN);
    expect(kindOf(await client.call("status", {}).catch((e: unknown) => e))).toBe("not-ready");
    expect(kindOf(await client.call("signer.lock", {}).catch((e: unknown) => e))).toBe("auth");
    expect(kindOf(await client.call("vault.list", {}).catch((e: unknown) => e))).toBe(
      "method-not-found",
    );
    expect(ERROR_CODES["not-ready"]).toBe(-32010);
    client.close();
  });

  it("delivers numbered events to subscribed sessions only", async () => {
    const subscribed = await DaemonClient.connect(socket, TOKEN);
    const other = await DaemonClient.connect(socket, TOKEN);
    const seen: EventEnvelope[] = [];
    const { subscription } = await subscribed.subscribe((_, envelope) => seen.push(envelope));
    expect(subscription).toMatchObject({ latest: 0, reset: false });
    server.broadcast(changed(1));
    // The unsubscribed session still answers calls, and no event line confuses it.
    expect(await other.call("daemon.info", {})).toEqual({ method: "daemon.info" });
    await settle();
    expect(seen.map((e) => [e.seq.n, e.event])).toEqual([[1, changed(1)]]);
    expect(seen[0]?.seq.epoch).toBe(subscription?.epoch);
    subscribed.close();
    other.close();
  });

  it("replays what a reconnecting session missed", async () => {
    const first = await DaemonClient.connect(socket, TOKEN);
    const { subscription } = await first.subscribe(() => {});
    server.broadcast(changed(1));
    server.broadcast(changed(2));
    server.broadcast(changed(3));
    first.close();
    const epoch = subscription?.epoch ?? 0;

    const again = await DaemonClient.connect(socket, TOKEN);
    const seen: number[] = [];
    const resumed = await again.subscribe((_, envelope) => seen.push(envelope.seq.n), {
      epoch,
      n: 1,
    });
    await settle();
    expect(resumed.subscription).toEqual({ epoch, latest: 3, reset: false });
    expect(seen).toEqual([2, 3]);
    server.broadcast(changed(4));
    await settle();
    expect(seen).toEqual([2, 3, 4]);
    again.close();
  });

  it("tells a session it can't catch up to refetch", async () => {
    for (let n = 1; n <= 1030; n += 1) server.broadcast(changed(n));
    const client = await DaemonClient.connect(socket, TOKEN);
    const seen: number[] = [];
    const tooOld = await client.subscribe((_, envelope) => seen.push(envelope.seq.n), {
      epoch: (await client.subscribe(() => {})).subscription?.epoch ?? 0,
      n: 2,
    });
    const otherEpoch = await client.subscribe(() => {}, { epoch: 1, n: 1 });
    await settle();
    expect(tooOld.subscription?.reset).toBe(true);
    expect(otherEpoch.subscription?.reset).toBe(true);
    expect(seen).toEqual([]);
    client.close();
  });

  it("keeps an error's details, also from a wrapped cause", async () => {
    const client = await DaemonClient.connect(socket, TOKEN);
    const error = (await client
      .call("signer.status", {})
      .catch((e: unknown) => e)) as DaemonCallError;
    expect(error.kind).toBe("unreadable");
    expect(error.details).toEqual({ kind: "unreadable", answered: 1, needed: 2 });
    client.close();
  });
});

const changed = (n: number): DaemonEvent => ({
  type: "balanceChanged",
  vault: `0x${"11".repeat(32)}`,
  token: `0x${n.toString(16).padStart(64, "0")}`,
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 50));
