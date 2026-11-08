import type { SponsoredFeePaymentMethod } from "@aztec-labs/aztec.js/fee";
import type { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { DaemonEvent } from "@ome-labs/api";
import type { OmeWallet } from "@ome-labs/wallet";

import type { KeeperSupervisor } from "./keeper/supervisor.ts";
import type { NodeService } from "./node/node.ts";
import type { ProfilePaths } from "./profile/paths.ts";
import type { ProfileState } from "./profile/state.ts";
import type { SecretsStore } from "./secrets/store.ts";
import type { SignerService } from "./signer/signer.ts";
import type { VaultService } from "./vaults/service.ts";

/** What the daemon's services share. Accessors throw `not-ready` until the part they return is up. */
export interface Context {
  readonly paths: ProfilePaths;
  readonly state: ProfileState;
  readonly signer: SignerService;
  readonly keeper: KeeperSupervisor;
  readonly node: NodeService;
  readonly vaults: VaultService;
  wallet(): OmeWallet;
  secrets(): SecretsStore;
  /** The member's own Aztec account, which sends deployments. */
  account(): AztecAddress;
  /** The fee payer this profile trusts, and how its transactions pay through it. */
  feePayer(): AztecAddress;
  fee(): { paymentMethod: SponsoredFeePaymentMethod };
  emit(event: DaemonEvent): void;
}

/** Polls `check` every `intervalMs` until it returns a value, or throws after `timeoutMs`. */
export async function until<T>(
  check: () => Promise<T | undefined>,
  what: string,
  timeoutMs = 600_000,
  intervalMs = 500,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    // oxlint-disable-next-line no-await-in-loop
    const value = await check();
    if (value !== undefined) return value;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
}
