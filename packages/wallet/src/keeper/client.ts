import { type Socket, createConnection } from "node:net";
import { createInterface } from "node:readline";

import type {
  ErrorKind,
  KeeperEvent,
  KeeperMethod,
  KeeperParams,
  KeeperResult,
  RpcError,
} from "@ome-labs/api";

/** An error the keeper returned, with its API kind. */
export class KeeperError extends Error {
  readonly kind: ErrorKind;
  readonly code: number;

  constructor(error: RpcError) {
    super(error.message);
    this.name = "KeeperError";
    this.kind = error.data?.kind ?? "failed";
    this.code = error.code;
  }
}

interface Pending {
  readonly resolve: (value: unknown) => void;
  readonly reject: (error: Error) => void;
}

interface Reply {
  id?: number;
  result?: unknown;
  error?: RpcError;
  method?: string;
  params?: unknown;
}

/**
 * Typed JSON-RPC client of the member's own keeper over its local socket (newline-delimited JSON, `ome_api::keeper`).
 * The session presents the keeper's launch token before any call; calls run concurrently.
 */
export class KeeperClient {
  readonly #socket: Socket;
  readonly #pending = new Map<number, Pending>();
  readonly #listeners = new Set<(event: KeeperEvent) => void>();
  #next = 0;
  #closed: Error | undefined;

  private constructor(socket: Socket) {
    this.#socket = socket;
    createInterface({ input: socket, crlfDelay: Number.POSITIVE_INFINITY }).on("line", (line) =>
      this.#receive(line),
    );
    socket.on("close", () => this.#fail(new Error("keeper connection closed")));
    socket.on("error", (error) => this.#fail(error));
  }

  /** Connects to the keeper's socket and authenticates with `token`. */
  static async connect(socketPath: string, token: string): Promise<KeeperClient> {
    const socket = await new Promise<Socket>((resolve, reject) => {
      const s = createConnection(socketPath, () => resolve(s));
      s.once("error", reject);
    });
    const client = new KeeperClient(socket);
    await client.call("session.auth", { token });
    return client;
  }

  /** Calls keeper method `method`; rejects with {@link KeeperError} on a keeper error. */
  call<M extends KeeperMethod>(method: M, params: KeeperParams<M>): Promise<KeeperResult<M>> {
    if (this.#closed) return Promise.reject(this.#closed);
    this.#next += 1;
    const id = this.#next;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
      this.#socket.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });
  }

  /** Streams keeper events to `listener`; returns an unsubscribe function. */
  async subscribe(listener: (event: KeeperEvent) => void): Promise<() => void> {
    if (this.#listeners.size === 0) await this.call("events.subscribe", {});
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  close(): void {
    this.#socket.end();
  }

  #receive(line: string): void {
    let message: Reply;
    try {
      message = JSON.parse(line) as Reply;
    } catch {
      return;
    }
    if (message.method === "event") {
      for (const listener of this.#listeners) listener(message.params as KeeperEvent);
      return;
    }
    if (message.id === undefined) return;
    const pending = this.#pending.get(message.id);
    if (!pending) return;
    this.#pending.delete(message.id);
    if (message.error) pending.reject(new KeeperError(message.error));
    else pending.resolve(message.result ?? null);
  }

  #fail(error: Error): void {
    this.#closed ??= error;
    for (const { reject } of this.#pending.values()) reject(error);
    this.#pending.clear();
  }
}
