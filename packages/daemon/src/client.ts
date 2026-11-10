import { readFile } from "node:fs/promises";
import { type Socket, createConnection } from "node:net";
import { join } from "node:path";
import { createInterface } from "node:readline";

import type {
  DaemonEvent,
  DaemonMethod,
  DaemonParams,
  DaemonResult,
  ErrorDetails,
  ErrorKind,
  EventEnvelope,
  EventSeq,
  RpcError,
  Subscription,
} from "@ome-labs/api";

import { omeHome } from "./profile/paths.ts";

/** An error the daemon returned, with its API kind. */
export class DaemonCallError extends Error {
  readonly kind: ErrorKind;
  readonly details: ErrorDetails | undefined;

  constructor(error: RpcError) {
    super(error.message);
    this.name = "DaemonCallError";
    this.kind = error.data?.kind ?? "failed";
    this.details = error.data?.details;
  }
}

/** Receives each event, with its number and time. */
export type EventListener = (event: DaemonEvent, envelope: EventEnvelope) => void;

/** Typed client of a daemon (`ome_api::daemon`), for TypeScript apps and tests; Rust apps use `ome-client`. */
export class DaemonClient {
  readonly #socket: Socket;
  readonly #pending = new Map<
    number,
    { resolve: (v: unknown) => void; reject: (e: Error) => void }
  >();
  readonly #listeners = new Set<EventListener>();
  #subscribed = false;
  #next = 0;

  private constructor(socket: Socket) {
    this.#socket = socket;
    createInterface({ input: socket }).on("line", (line) => {
      const message = JSON.parse(line) as {
        id?: number;
        result?: unknown;
        error?: RpcError;
        method?: string;
        params?: unknown;
      };
      if (message.method === "event") {
        const envelope = message.params as EventEnvelope;
        for (const listener of this.#listeners) listener(envelope.event, envelope);
        return;
      }
      const pending = message.id === undefined ? undefined : this.#pending.get(message.id);
      if (!pending || message.id === undefined) return;
      this.#pending.delete(message.id);
      if (message.error) pending.reject(new DaemonCallError(message.error));
      else pending.resolve(message.result);
    });
    socket.on("close", () => {
      for (const { reject } of this.#pending.values())
        reject(new Error("daemon connection closed"));
      this.#pending.clear();
    });
  }

  static async connect(socketPath: string, token: string): Promise<DaemonClient> {
    const socket = await new Promise<Socket>((resolve, reject) => {
      const s = createConnection(socketPath, () => resolve(s));
      s.once("error", reject);
    });
    const client = new DaemonClient(socket);
    try {
      await client.call("session.auth", { token });
    } catch (error) {
      client.close();
      throw error;
    }
    return client;
  }

  /** Connects to profile `name`'s running daemon through its `daemon.addr` and `daemon.token`. */
  static async discover(name: string, home = omeHome()): Promise<DaemonClient> {
    const dir = join(home, "profiles", name);
    const addr = (await readFile(join(dir, "daemon.addr"), "utf8")).trim();
    const token = (await readFile(join(dir, "daemon.token"), "utf8")).trim();
    return DaemonClient.connect(addr.startsWith("/") ? addr : join(dir, addr), token);
  }

  call<M extends DaemonMethod>(method: M, params: DaemonParams<M>): Promise<DaemonResult<M>> {
    this.#next += 1;
    const id = this.#next;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.#socket.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });
  }

  /**
   * Calls `listener` with every event from now on. With `since` (the last event a previous session saw), the daemon
   * first replays the ones after it, or answers `reset` when it can't; the answer is returned either way.
   */
  async subscribe(
    listener: EventListener,
    since?: EventSeq,
  ): Promise<{ stop: () => void; subscription: Subscription | undefined }> {
    // Added before asking: replayed events arrive right after the answer.
    this.#listeners.add(listener);
    const stop = () => {
      this.#listeners.delete(listener);
    };
    if (this.#subscribed && !since) return { stop, subscription: undefined };
    try {
      const subscription = await this.call("events.subscribe", since ? { since } : {});
      this.#subscribed = true;
      return { stop, subscription };
    } catch (error) {
      stop();
      throw error;
    }
  }

  close(): void {
    this.#socket.end();
  }
}
