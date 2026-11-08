import { type ChildProcess, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import { open } from "node:fs/promises";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import { API_VERSION, type KeeperState } from "@ome-labs/api";
import { KeeperClient } from "@ome-labs/wallet";

import { DaemonError } from "../errors.ts";
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
  if (!found) throw new DaemonError("not-ready", "ome-keeper not found; set OME_KEEPER_BIN");
  return found;
}

interface Ready {
  identity: string;
  transport: string;
  peerId: string;
  addresses: string[];
  rpcSocket: string;
}

/**
 * Runs the member's keeper as a child process: hands it the master key and a fresh session token on stdin (never on
 * the command line or in the environment), connects, checks its API version, and restarts it with backoff when it
 * exits. The keeper holds the viewing share; this process never does.
 */
export class KeeperSupervisor {
  readonly #binary: string;
  readonly #onChange: (state: KeeperState) => void;
  #options: KeeperOptions | undefined;
  #masterKey: Buffer | undefined;
  #child: ChildProcess | undefined;
  #client: KeeperClient | undefined;
  #state: KeeperState = {
    status: "stopped",
    identity: null,
    peerId: null,
    addresses: [],
    version: null,
    error: null,
  };
  #failures = 0;
  #stopping = false;
  #timer: NodeJS.Timeout | undefined;

  constructor(binary: string, onChange: (state: KeeperState) => void) {
    this.#binary = binary;
    this.#onChange = onChange;
  }

  get state(): KeeperState {
    return this.#state;
  }

  /** The keeper's client; throws `not-ready` until it runs. */
  get client(): KeeperClient {
    if (!this.#client) throw new DaemonError("not-ready", `the keeper is ${this.#state.status}`);
    return this.#client;
  }

  async start(options: KeeperOptions, masterKey: Buffer): Promise<void> {
    this.#options = options;
    this.#masterKey = masterKey;
    this.#stopping = false;
    await this.#launch();
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

  async #launch(): Promise<void> {
    const options = this.#options;
    const masterKey = this.#masterKey;
    if (!options || !masterKey) return;
    this.#set({ ...this.#state, status: this.#failures ? "restarting" : "starting", error: null });
    try {
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
      const child = spawn(this.#binary, args, {
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
      const client = await KeeperClient.connect(ready.rpcSocket, token);
      const info = await client.call("keeper.info", {});
      if (info.apiVersion !== API_VERSION) {
        throw new Error(
          `the keeper speaks API version ${info.apiVersion}, this daemon ${API_VERSION}`,
        );
      }
      this.#child = child;
      this.#client = client;
      this.#failures = 0;
      child.once("exit", (code) => this.#exited(child, code));
      this.#set({
        status: "running",
        identity: ready.identity,
        peerId: ready.peerId,
        addresses: ready.addresses,
        version: info.version,
        error: null,
      });
    } catch (error) {
      this.#scheduleRestart(error instanceof Error ? error.message : String(error));
    }
  }

  #exited(child: ChildProcess, code: number | null): void {
    if (this.#child !== child || this.#stopping) return;
    this.#child = undefined;
    this.#client = undefined;
    this.#scheduleRestart(`the keeper stopped unexpectedly (exit code ${code ?? "unknown"})`);
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
