import { type ChildProcess, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { open } from "node:fs/promises";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import { API_VERSION, type KeeperEvent, type KeeperState } from "@ome-labs/api";
import { KeeperClient } from "@ome-labs/wallet";

import { notReady } from "../errors.ts";
import { privateDir } from "../profile/files.ts";

/** Listen on every interface, so members on other machines can dial in; ports are picked by the OS. */
const LISTEN = ["/ip4/0.0.0.0/udp/0/quic-v1", "/ip4/0.0.0.0/tcp/0"];
const MAX_BACKOFF_MS = 30_000;

export interface KeeperOptions {
  readonly dataDir: string;
  readonly socket: string;
  readonly relays: readonly string[];
  readonly log: string;
  readonly listen?: readonly string[];
}

/** `OME_KEEPER_BIN`, then `<home>/bin/ome-keeper`, then the repository's debug build. */
export function keeperBinary(home: string): string {
  const candidates = [
    process.env.OME_KEEPER_BIN,
    join(home, "bin", "ome-keeper"),
    fileURLToPath(new URL("../../../../crates/target/debug/ome-keeper", import.meta.url)),
  ].filter((path): path is string => Boolean(path));
  const found = candidates.find((path) => existsSync(path));
  if (!found) throw notReady("keeper", "ome-keeper not found; set OME_KEEPER_BIN");
  return found;
}

interface Ready {
  identity: string;
  transport: string;
  peerId: string;
  addresses: string[];
  rpcSocket: string;
}

/** A keeper on another machine and its RPC token. */
export interface RemoteTarget {
  readonly address: string;
  readonly fingerprint: string;
  readonly token: string;
}

type Mode =
  | { readonly kind: "local"; readonly options: KeeperOptions; readonly masterKey: Buffer }
  | { readonly kind: "remote"; readonly target: RemoteTarget };

/**
 * The member's keeper, in one of two modes:
 *
 * - **local**: runs it as a child process, hands it the master key and a fresh session token on stdin (never on the
 *   command line or in the environment), connects to its socket, and restarts it with backoff when it exits;
 * - **remote**: connects over TLS to a keeper on the member's own server (`ome-keeper run --rpc-listen`), pinning its
 *   certificate's fingerprint, and reconnects with backoff when the connection drops.
 *
 * Either way it checks the keeper's API version, and every event listener is subscribed again on each new
 * connection. The keeper holds the viewing share; this process never does.
 */
export class KeeperSupervisor {
  readonly #binary: () => string;
  readonly #onChange: (state: KeeperState) => void;
  readonly #listeners = new Set<(event: KeeperEvent) => void>();
  #mode: Mode | undefined;
  #child: ChildProcess | undefined;
  #client: KeeperClient | undefined;
  #state: KeeperState = {
    status: "stopped",
    identity: null,
    peerId: null,
    addresses: [],
    version: null,
    error: null,
    remote: null,
  };
  #failures = 0;
  #stopping = false;
  #timer: NodeJS.Timeout | undefined;

  constructor(binary: () => string, onChange: (state: KeeperState) => void) {
    this.#binary = binary;
    this.#onChange = onChange;
  }

  get state(): KeeperState {
    return this.#state;
  }

  /** The keeper's client; throws `not-ready` until it runs. */
  get client(): KeeperClient {
    if (!this.#client) throw notReady("keeper", `the keeper is ${this.#state.status}`);
    return this.#client;
  }

  /** Streams the keeper's events to `listener`, across restarts and reconnections. */
  async subscribe(listener: (event: KeeperEvent) => void): Promise<void> {
    this.#listeners.add(listener);
    await this.#client?.subscribe(listener);
  }

  /** Runs the daemon's own keeper. */
  async start(options: KeeperOptions, masterKey: Buffer): Promise<void> {
    await this.#switch({ kind: "local", options, masterKey });
  }

  /** Uses a keeper on another machine. */
  async startRemote(target: RemoteTarget): Promise<void> {
    await this.#switch({ kind: "remote", target });
  }

  async stop(): Promise<void> {
    this.#stopping = true;
    clearTimeout(this.#timer);
    this.#client?.close();
    this.#client = undefined;
    const child = this.#child;
    this.#child = undefined;
    if (child && child.exitCode === null) {
      const exited = new Promise((resolve) => child.once("exit", resolve));
      child.kill("SIGTERM");
      await exited;
    }
    this.#set({ ...this.#state, status: "stopped" });
  }

  async #switch(mode: Mode): Promise<void> {
    if (this.#mode) await this.stop();
    this.#mode = mode;
    this.#stopping = false;
    this.#failures = 0;
    await this.#launch();
  }

  async #launch(): Promise<void> {
    const mode = this.#mode;
    if (!mode) return;
    this.#set({ ...this.#state, status: this.#failures ? "restarting" : "starting", error: null });
    try {
      const { client, child, remote } =
        mode.kind === "local"
          ? { ...(await this.#spawn(mode.options, mode.masterKey)), remote: null }
          : {
              client: await KeeperClient.connectRemote(
                mode.target.address,
                mode.target.fingerprint,
                mode.target.token,
              ),
              child: undefined,
              remote: mode.target.address,
            };
      const info = await client.call("keeper.info", {});
      if (info.apiVersion !== API_VERSION) {
        client.close();
        throw new Error(
          `the keeper speaks API version ${info.apiVersion}, this daemon ${API_VERSION}`,
        );
      }
      for (const listener of this.#listeners)
        // oxlint-disable-next-line no-await-in-loop
        await client.subscribe(listener);
      this.#child = child;
      this.#client = client;
      this.#failures = 0;
      if (child)
        child.once("exit", (code) =>
          this.#lost(client, `the keeper stopped unexpectedly (exit code ${code ?? "unknown"})`),
        );
      else
        client.onClose((error) => this.#lost(client, `lost the remote keeper: ${error.message}`));
      this.#set({
        status: "running",
        identity: info.identity,
        peerId: info.peerId,
        addresses: info.addresses,
        version: info.version,
        error: null,
        remote,
      });
    } catch (error) {
      this.#scheduleRestart(error instanceof Error ? error.message : String(error));
    }
  }

  /** Spawns `ome-keeper run` and connects to its socket. */
  async #spawn(
    options: KeeperOptions,
    masterKey: Buffer,
  ): Promise<{ client: KeeperClient; child: ChildProcess }> {
    await privateDir(options.dataDir);
    const log = await open(options.log, "a", 0o600);
    const token = randomBytes(32).toString("hex");
    const args = [
      "run",
      "--data-dir",
      options.dataDir,
      "--secrets-stdin",
      "--rpc-socket",
      options.socket,
    ];
    for (const address of options.listen ?? LISTEN) args.push("--listen", address);
    for (const relay of options.relays) args.push("--relay", relay);
    const child = spawn(this.#binary(), args, {
      stdio: ["pipe", "pipe", log.fd],
      env: { ...process.env, RUST_LOG: process.env.RUST_LOG ?? "info", NO_COLOR: "1" },
    });
    await log.close();
    const { stdin, stdout } = child;
    if (!stdin || !stdout) throw new Error("the keeper started without pipes");
    stdin.end(`${JSON.stringify({ masterKey: masterKey.toString("hex"), rpcToken: token })}\n`);
    const ready = await new Promise<Ready>((resolve, reject) => {
      createInterface({ input: stdout }).once("line", (line) => {
        try {
          resolve((JSON.parse(line) as { ready: Ready }).ready);
        } catch (error) {
          reject(error);
        }
      });
      child.once("exit", (code) =>
        reject(new Error(`ome-keeper exited with ${code} before it was ready`)),
      );
    });
    return { client: await KeeperClient.connect(ready.rpcSocket, token), child };
  }

  #lost(client: KeeperClient, error: string): void {
    if (this.#client !== client || this.#stopping) return;
    this.#child = undefined;
    this.#client = undefined;
    this.#scheduleRestart(error);
  }

  #scheduleRestart(error: string): void {
    if (this.#stopping) return;
    this.#failures += 1;
    const delay = Math.min(MAX_BACKOFF_MS, 500 * 2 ** this.#failures);
    this.#set({ ...this.#state, status: this.#failures > 5 ? "failed" : "restarting", error });
    this.#timer = setTimeout(() => void this.#launch(), delay);
    this.#timer.unref();
  }

  #set(state: KeeperState): void {
    this.#state = state;
    this.#onChange(state);
  }
}
