// The app page `omed` serves on 127.0.0.1 when started with an apps port (`OME_APPS_PORT`): a development and test
// transport for app connections. It serves the page an app loads in an iframe, wallet-sdk's crypto and message-type
// modules, and the two calls the page makes. Those need the `x-ome-page` header, which a website can't send here
// without a CORS preflight this server never answers, so only the page itself reaches them.
import { readFile } from "node:fs/promises";
import { type IncomingMessage, type Server, type ServerResponse, createServer } from "node:http";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import type { AppService } from "./service.ts";

const PAGE = fileURLToPath(new URL("./page/", import.meta.url));
const SDK = dirname(fileURLToPath(import.meta.resolve("@aztec-labs/wallet-sdk/crypto")));

/** What the page and wallet-sdk modules are, by path. */
const FILES: Record<string, { dir: string; file: string; type: string }> = {
  "/": { dir: PAGE, file: "index.html", type: "text/html; charset=utf-8" },
  "/main.js": { dir: PAGE, file: "main.js", type: "text/javascript" },
  "/wallet-page.js": { dir: PAGE, file: "wallet-page.js", type: "text/javascript" },
  "/sdk/crypto.js": { dir: SDK, file: "crypto.js", type: "text/javascript" },
  "/sdk/types.js": { dir: SDK, file: "types.js", type: "text/javascript" },
  "/sdk/emoji_alphabet.js": { dir: SDK, file: "emoji_alphabet.js", type: "text/javascript" },
};

/** Largest call body: a transaction with its contracts' calls, never an artifact. */
const MAX_BODY = 4 * 1024 * 1024;

export class AppPageServer {
  readonly #server: Server;
  readonly origin: string;

  private constructor(server: Server, origin: string) {
    this.#server = server;
    this.origin = origin;
  }

  /** Listens on 127.0.0.1:`port` (0 picks one). */
  static async listen(port: number, apps: AppService): Promise<AppPageServer> {
    let origin = "";
    const server = createServer((request, response) => {
      void handle(apps, origin, request, response).catch((error: unknown) =>
        send(response, 500, { error: error instanceof Error ? error.message : String(error) }),
      );
    });
    // An app's transaction waits for the members: its call stays open far past Node's default request timeout.
    server.requestTimeout = 0;
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(port, "127.0.0.1", resolve);
    });
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("the app page has no port");
    origin = `http://127.0.0.1:${address.port}`;
    return new AppPageServer(server, origin);
  }

  close(): Promise<void> {
    this.#server.closeAllConnections();
    return new Promise((resolve) => this.#server.close(() => resolve()));
  }
}

async function handle(
  apps: AppService,
  origin: string,
  request: IncomingMessage,
  response: ServerResponse,
): Promise<void> {
  const url = new URL(request.url ?? "/", origin);
  const file = FILES[url.pathname];
  if (request.method === "GET" && file) {
    response.writeHead(200, { "content-type": file.type, "cache-control": "no-store" });
    response.end(await readFile(join(file.dir, file.file)));
    return;
  }
  if (!url.pathname.startsWith("/api/")) return send(response, 404, { error: "not found" });
  const from = request.headers.origin;
  if (request.headers["x-ome-page"] !== "1" || (from !== undefined && from !== origin))
    return send(response, 403, { error: "only the Ome page calls this" });
  if (request.method === "GET" && url.pathname === "/api/allowed") {
    const app = url.searchParams.get("origin") ?? "";
    let allowed = true;
    try {
      apps.app(app);
    } catch {
      allowed = false;
    }
    return send(response, 200, { allowed });
  }
  if (request.method === "POST" && url.pathname === "/api/call") {
    const body = JSON.parse(await read(request)) as {
      origin: string;
      method: string;
      args: string;
    };
    try {
      const { result } = await apps.call(body);
      return send(response, 200, { result: JSON.parse(result) as unknown });
    } catch (error) {
      return send(response, 200, { error: error instanceof Error ? error.message : String(error) });
    }
  }
  return send(response, 404, { error: "not found" });
}

function send(response: ServerResponse, status: number, body: unknown): void {
  if (response.headersSent) return;
  response.writeHead(status, { "content-type": "application/json", "cache-control": "no-store" });
  response.end(JSON.stringify(body));
}

async function read(request: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    size += (chunk as Buffer).length;
    if (size > MAX_BODY) throw new Error("the call is too large");
    chunks.push(chunk as Buffer);
  }
  return Buffer.concat(chunks).toString("utf8");
}
