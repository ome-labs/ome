import { timingSafeEqual } from "node:crypto";
import { chmod, rm } from "node:fs/promises";
import { type Server, type Socket, createServer } from "node:net";
import { createInterface } from "node:readline";

import type { DaemonEvent, DaemonMethod } from "@ome-labs/api";

import { DaemonError, toRpcError } from "../errors.ts";

/** Runs one method; `params` are as the client sent them (missing params read as `{}`). */
export type Handler = (method: DaemonMethod, params: unknown) => Promise<unknown>;

/** Largest accepted request line. */
const MAX_LINE = 1 << 20;

interface Request {
  readonly jsonrpc?: unknown;
  readonly id?: number | string | null;
  readonly method?: unknown;
  readonly params?: unknown;
}

/**
 * The daemon API on an owner-only Unix socket: newline-delimited JSON-RPC 2.0, the keeper's framing. The first call of
 * every connection is `session.auth { token }`, compared in constant time; after `events.subscribe` the connection
 * also receives `event` notifications.
 */
export class DaemonServer {
  readonly #server: Server;
  readonly #token: Buffer;
  readonly #methods: ReadonlySet<string>;
  readonly #subscribers = new Set<Socket>();
  readonly #connections = new Set<Socket>();

  private constructor(token: string, methods: ReadonlySet<string>, handle: Handler) {
    this.#token = Buffer.from(token, "utf8");
    this.#methods = methods;
    this.#server = createServer((socket) => this.#connection(socket, handle));
  }

  static async listen(
    path: string,
    token: string,
    methods: ReadonlySet<string>,
    handle: Handler,
  ): Promise<DaemonServer> {
    await rm(path, { force: true });
    const server = new DaemonServer(token, methods, handle);
    await new Promise<void>((resolve, reject) => {
      server.#server.once("error", reject);
      server.#server.listen(path, () => resolve());
    });
    await chmod(path, 0o600);
    return server;
  }

  /** Sends `event` to every subscribed connection. */
  broadcast(event: DaemonEvent): void {
    const line = `${JSON.stringify({ jsonrpc: "2.0", method: "event", params: event })}\n`;
    for (const socket of this.#subscribers) socket.write(line);
  }

  close(): Promise<void> {
    const closed = new Promise<void>((resolve) => this.#server.close(() => resolve()));
    for (const socket of this.#connections) socket.destroy();
    return closed;
  }

  #connection(socket: Socket, handle: Handler): void {
    let authenticated = false;
    const reply = (id: Request["id"], body: { result: unknown } | { error: unknown }) => {
      if (id === undefined || socket.destroyed) return;
      socket.write(`${JSON.stringify({ jsonrpc: "2.0", id, ...body })}\n`);
    };
    const fail = (id: Request["id"], error: DaemonError) => reply(id, { error: toRpcError(error) });
    this.#connections.add(socket);
    const forget = () => {
      this.#subscribers.delete(socket);
      this.#connections.delete(socket);
    };
    socket.on("error", forget);
    socket.on("close", forget);
    const lines = createInterface({ input: socket, crlfDelay: Number.POSITIVE_INFINITY });
    lines.on("line", (line) => {
      if (line.length > MAX_LINE) {
        socket.destroy();
        return;
      }
      let request: Request;
      try {
        request = JSON.parse(line) as Request;
      } catch {
        fail(null, new DaemonError("parse", "parse error"));
        return;
      }
      if (request.method === "session.auth") {
        authenticated = this.#matches((request.params as { token?: unknown } | null)?.token);
        if (authenticated) reply(request.id, { result: null });
        else fail(request.id, new DaemonError("unauthorized", "invalid token"));
        return;
      }
      if (!authenticated) {
        fail(request.id, new DaemonError("unauthorized", "authenticate with session.auth first"));
        return;
      }
      if (request.jsonrpc !== "2.0" || typeof request.method !== "string") {
        fail(request.id, new DaemonError("invalid-request", "not a JSON-RPC 2.0 request"));
        return;
      }
      if (request.method === "events.subscribe") {
        this.#subscribers.add(socket);
        reply(request.id, { result: null });
        return;
      }
      if (!this.#methods.has(request.method)) {
        fail(request.id, new DaemonError("method-not-found", `unknown method ${request.method}`));
        return;
      }
      const method = request.method as DaemonMethod;
      void (async () => {
        try {
          reply(request.id, { result: (await handle(method, request.params ?? {})) ?? null });
        } catch (error) {
          reply(request.id, { error: toRpcError(error) });
        }
      })();
    });
  }

  #matches(given: unknown): boolean {
    if (typeof given !== "string") return false;
    const candidate = Buffer.from(given, "utf8");
    return candidate.length === this.#token.length && timingSafeEqual(candidate, this.#token);
  }
}
