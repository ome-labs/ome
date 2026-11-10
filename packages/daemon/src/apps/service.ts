// App connections: which apps may use which vault, and the entry point for their wallet calls. Off until the member
// turns it on (`config.set { appsEnabled }`); an app the member hasn't allowed is refused, and an `appConnect` event
// tells the member it asked.
import { WalletSchema } from "@aztec-labs/aztec.js/wallet";
import { jsonStringify } from "@aztec-labs/foundation/json-rpc";
import {
  getSchemaParameters,
  parseWithOptionals,
  schemaHasMethod,
} from "@aztec-labs/foundation/schemas";
import type { AppAllowParams, AppCallParams, AppCallResult, AppView, Hex32 } from "@ome-labs/api";
import type { VaultAuthwit } from "@ome-labs/wallet";

import type { Context } from "../context.ts";
import { DaemonError, require } from "../errors.ts";
import type { AppRecord } from "../profile/state.ts";
import type { ProposalService } from "../proposals/service.ts";
import { AppWallet } from "./wallet.ts";

/** A web origin: scheme, host and optional port, nothing else. */
const ORIGIN = /^https?:\/\/[a-z0-9.-]+(:[0-9]{1,5})?$|^https?:\/\/\[[0-9a-f:]+\](:[0-9]{1,5})?$/u;

/** The `Wallet` methods an app may call. */
const METHODS = new Set([
  "getChainInfo",
  "getContractMetadata",
  "getContractClassMetadata",
  "registerSender",
  "getAddressBook",
  "getAccounts",
  "registerContract",
  "registerContractClass",
  "simulateTx",
  "executeUtility",
  "profileTx",
  "sendTx",
  "createAuthWit",
  "requestCapabilities",
  "batch",
  "getPrivateEvents",
]);

export class AppService {
  readonly #context: Context;
  readonly #proposals: ProposalService;
  /** Each app's authwits waiting for its next transaction. */
  readonly #pending = new Map<string, VaultAuthwit[]>();

  constructor(context: Context, proposals: ProposalService) {
    this.#context = context;
    this.#proposals = proposals;
  }

  list(): AppView[] {
    return Object.entries(this.#context.state.file.apps).map(([origin, app]) => view(origin, app));
  }

  async allow(params: AppAllowParams): Promise<AppView> {
    const origin = this.#origin(params.origin);
    const vault = params.vault.toLowerCase() as Hex32;
    this.#context.vaults.record(vault);
    const name = params.name ?? null;
    require(name === null ||
      (name.length > 0 && Buffer.byteLength(name) <= 64), "an app's name is 1 to 64 bytes");
    const app: AppRecord = { vault, name, allowedAt: Math.floor(Date.now() / 1000) };
    await this.#context.state.update((file) => {
      file.apps[origin] = app;
    });
    return view(origin, app);
  }

  async revoke(origin: string): Promise<void> {
    const key = this.#origin(origin);
    if (!this.#context.state.file.apps[key])
      throw new DaemonError("not-found", `${key} isn't an allowed app`);
    this.#pending.delete(key);
    await this.#context.state.update((file) => {
      delete file.apps[key];
    });
  }

  /** The app at `origin`, when app connections are on and the member allowed it. */
  app(origin: string): AppRecord {
    const key = this.#origin(origin);
    if (!this.#context.state.file.settings.appsEnabled)
      throw new DaemonError(
        "invalid",
        "app connections are off: turn them on with config.set appsEnabled",
      );
    const app = this.#context.state.file.apps[key];
    if (!app) {
      this.#context.emit({ type: "appConnect", origin: key });
      throw new DaemonError(
        "invalid",
        `${key} isn't allowed to use a vault: allow it with apps.allow`,
      );
    }
    return app;
  }

  /** One wallet call: arguments parsed with aztec.js's `WalletSchema`, the result encoded as aztec.js does. */
  async call(params: AppCallParams): Promise<AppCallResult> {
    const origin = this.#origin(params.origin);
    const app = this.app(origin);
    require(METHODS.has(params.method) &&
      schemaHasMethod(WalletSchema, params.method), `${params.method} isn't a wallet method`);
    let raw: unknown;
    try {
      raw = JSON.parse(params.args);
    } catch {
      throw new DaemonError("invalid", "an app call's arguments are a JSON list");
    }
    require(Array.isArray(raw), "an app call's arguments are a JSON list");
    const schema = WalletSchema[params.method as keyof typeof WalletSchema];
    const args = (await parseWithOptionals(raw, getSchemaParameters(schema))) as unknown[];
    const pending = this.#pending.get(origin) ?? [];
    this.#pending.set(origin, pending);
    const wallet = new AppWallet(this.#context, this.#proposals, origin, app, pending);
    const method = (wallet as unknown as Record<string, (...a: unknown[]) => Promise<unknown>>)[
      params.method
    ];
    if (typeof method !== "function")
      throw new DaemonError("invalid", `${params.method} isn't a wallet method`);
    return { result: jsonStringify(await method.apply(wallet, args)) };
  }

  #origin(origin: string): string {
    const key = origin.toLowerCase();
    require(ORIGIN.test(key), `${origin} is not a web origin (https://host[:port])`);
    return key;
  }
}

function view(origin: string, app: AppRecord): AppView {
  return { origin, vault: app.vault, name: app.name, allowedAt: app.allowedAt };
}
