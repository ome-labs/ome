// The daemon's socket: sessions must authenticate, unknown methods and handler failures come back with the API's
// error kinds and codes, events reach subscribed sessions, and the socket is owner-only.
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { ERROR_CODES } from "@ome-labs/api";
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
      new Set(["daemon.info", "status", "signer.lock"]),
      async (method) => {
        if (method === "status") throw new DaemonError("not-ready", "starting");
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

  it("delivers events to subscribed sessions only", async () => {
    const subscribed = await DaemonClient.connect(socket, TOKEN);
    const other = await DaemonClient.connect(socket, TOKEN);
    const seen: unknown[] = [];
    await subscribed.subscribe((event) => seen.push(event));
    server.broadcast({ type: "vaultActivity", vault: `0x${"11".repeat(32)}` });
    // The unsubscribed session still answers calls, and no event line confuses it.
    expect(await other.call("daemon.info", {})).toEqual({ method: "daemon.info" });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(seen).toEqual([{ type: "vaultActivity", vault: `0x${"11".repeat(32)}` }]);
    subscribed.close();
    other.close();
  });
});
