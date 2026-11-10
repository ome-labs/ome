// Types of `wallet-page.js`, which the browser loads as is.
import type * as WalletCrypto from "@aztec-labs/wallet-sdk/crypto";

export interface WalletPageDeps {
  /** wallet-sdk's crypto module. */
  readonly crypto: typeof WalletCrypto;
  /** wallet-sdk's `WalletMessageType`. */
  readonly types: Record<string, string>;
  /** Posts a message to the app at `origin`. */
  post(message: object, origin: string): void;
  /** Whether the member allowed the app at `origin`. */
  allowed(origin: string): Promise<boolean>;
  /** One wallet call through the daemon: arguments as JSON, the decoded result back. */
  call(origin: string, method: string, args: string): Promise<unknown>;
  /** What the app shows for Ome. */
  readonly wallet: {
    readonly id: string;
    readonly name: string;
    readonly version: string;
    readonly icon?: string;
  };
}

export declare function createWalletPage(deps: WalletPageDeps): {
  /** Handles one message from the app at `origin`. */
  handle(message: unknown, origin: string): Promise<void>;
};
