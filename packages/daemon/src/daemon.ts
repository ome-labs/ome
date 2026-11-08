import { randomBytes } from "node:crypto";
import { readFile, rm } from "node:fs/promises";

import { SponsoredFeePaymentMethod } from "@aztec-labs/aztec.js/fee";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { GrumpkinScalar } from "@aztec-labs/foundation/curves/grumpkin";
import { SponsoredFPCContract } from "@aztec-labs/noir-contracts.js/SponsoredFPC";
import type { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { getContractInstanceFromInstantiationParams } from "@aztec-labs/stdlib/contract";
import {
  API_VERSION,
  type DaemonEvent,
  type DaemonInfo,
  type Hex32,
  type KeeperState,
  type ProfileCreateParams,
  type Secret,
  type Settings,
  type Status,
} from "@ome-labs/api";
import { OmeWallet } from "@ome-labs/wallet";

import type { Context } from "./context.ts";
import { DaemonError, require } from "./errors.ts";
import { FoundingService } from "./founding/founding.ts";
import { KeeperSupervisor, keeperBinary } from "./keeper/supervisor.ts";
import { MembershipService } from "./membership/service.ts";
import { NodeService } from "./node/node.ts";
import { privateDir, writePrivate } from "./profile/files.ts";
import { type ProfilePaths, omeHome, profilePaths } from "./profile/paths.ts";
import { ProfileState } from "./profile/state.ts";
import { ProposalService } from "./proposals/service.ts";
import { type MasterKeyStore, masterKeyStore } from "./secrets/master-key.ts";
import { SecretsStore } from "./secrets/store.ts";
import { handlers } from "./server/handlers.ts";
import { DaemonServer } from "./server/server.ts";
import { SignerService } from "./signer/signer.ts";
import { VaultService } from "./vaults/service.ts";

/** Release of this daemon. */
export const DAEMON_VERSION = "0.1.0";

const SYNC_INTERVAL_MS = 15_000;
const ACCOUNT_SECRET = "account";

export interface DaemonOptions {
  readonly profile: string;
  readonly home?: string;
  /** Overrides the profile's settings for this run (tests). */
  readonly settings?: Partial<Settings>;
  /** libp2p listen addresses for the keeper (tests listen on loopback). */
  readonly keeperListen?: readonly string[];
}

interface Account {
  secret: string;
  salt: string;
  signingKey: string;
}

/**
 * `omed`: one member's daemon for one profile. It serves the daemon API on an owner-only socket, keeps the sealed
 * signer, supervises the member's keeper, and hosts the wallet's PXE, through which it founds vaults, runs proposals
 * and membership changes, and tells the keeper what the chain says.
 */
export class Daemon implements Context {
  readonly paths: ProfilePaths;
  readonly signer: SignerService;
  readonly keeper: KeeperSupervisor;
  readonly vaults: VaultService;
  readonly founding: FoundingService;
  readonly proposals: ProposalService;
  readonly membership: MembershipService;
  readonly #options: DaemonOptions;
  readonly #startedAt = Math.floor(Date.now() / 1000);
  #state: ProfileState | undefined;
  #node: NodeService | undefined;
  #keys: MasterKeyStore | undefined;
  #secrets: SecretsStore | undefined;
  #wallet: OmeWallet | undefined;
  #account: AztecAddress | undefined;
  #feePayer: AztecAddress | undefined;
  #server: DaemonServer | undefined;
  #sync: NodeJS.Timeout | undefined;
  #core: Promise<void> | undefined;

  constructor(options: DaemonOptions) {
    this.#options = options;
    this.paths = profilePaths(options.profile, options.home ?? omeHome());
    this.signer = new SignerService(this.paths.signer);
    this.keeper = new KeeperSupervisor(keeperBinary(options.home ?? omeHome()), (state) =>
      this.#keeperChanged(state),
    );
    this.vaults = new VaultService(this);
    this.founding = new FoundingService(this);
    this.proposals = new ProposalService(this);
    this.membership = new MembershipService(this);
    this.proposals.rotationApprover = (vault, stored, tx) =>
      this.membership.approve(vault, stored, tx);
  }

  // Context.

  get state(): ProfileState {
    if (!this.#state) throw new DaemonError("not-ready", "the daemon is starting");
    return this.#state;
  }

  get node(): NodeService {
    if (!this.#node) throw new DaemonError("not-ready", "the daemon is starting");
    return this.#node;
  }

  wallet(): OmeWallet {
    if (!this.#wallet)
      throw new DaemonError(
        "not-ready",
        "the wallet is not running yet; create a profile or unlock",
      );
    return this.#wallet;
  }

  secrets(): SecretsStore {
    if (!this.#secrets) throw new DaemonError("not-ready", "unlock the signer first");
    return this.#secrets;
  }

  account(): AztecAddress {
    if (!this.#account)
      throw new DaemonError("not-ready", "the member's account is not loaded yet");
    return this.#account;
  }

  feePayer(): AztecAddress {
    if (!this.#feePayer) throw new DaemonError("not-ready", "the wallet is not running yet");
    return this.#feePayer;
  }

  fee() {
    return { paymentMethod: new SponsoredFeePaymentMethod(this.feePayer()) };
  }

  emit(event: DaemonEvent): void {
    this.#server?.broadcast(event);
  }

  // Lifecycle.

  /** Takes the profile lock, opens the socket and, with a profile whose keeper key is readable, starts the rest. */
  async start(): Promise<void> {
    await privateDir(this.paths.dir);
    await this.#lock();
    this.#state = await ProfileState.load(this.paths.state, this.#options.settings);
    this.#node = new NodeService(this.state.file.settings.nodeUrl);
    await this.node.check();
    await this.signer.load();
    this.#keys = await masterKeyStore(this.paths.name, this.paths.masterKey);
    const token = randomBytes(32).toString("hex");
    this.#server = await DaemonServer.listen(
      this.paths.socket,
      token,
      new Set(Object.keys(handlers)),
      (method, params) =>
        (
          (handlers as Record<string, (d: Daemon, p: unknown) => Promise<unknown>>)[method] as (
            d: Daemon,
            p: unknown,
          ) => Promise<unknown>
        )(this, params),
    );
    await writePrivate(this.paths.token, `${token}\n`);
    await writePrivate(this.paths.addr, `${this.paths.socket}\n`);
    if (this.state.file.profile) {
      const key = await this.#keys.read();
      if (key) await this.#startCore(key);
    }
  }

  async stop(): Promise<void> {
    clearInterval(this.#sync);
    await this.#server?.close();
    await this.#wallet?.stop();
    await this.keeper.stop();
    this.signer.lock();
    await Promise.all(
      [this.paths.addr, this.paths.token, this.paths.lock].map((p) => rm(p, { force: true })),
    );
  }

  info(): DaemonInfo {
    return {
      apiVersion: API_VERSION,
      version: DAEMON_VERSION,
      pid: process.pid,
      profile: this.paths.name,
      profileDir: this.paths.dir,
      startedAt: this.#startedAt,
      node: this.node.url,
    };
  }

  status(): Status {
    const { profile, settings, defaultVault } = this.state.file;
    const vaults = this.vaults
      .list()
      .filter((r) => r.status === "active")
      .flatMap((r) => {
        try {
          return [this.vaults.summary(r.address)];
        } catch {
          return [];
        }
      });
    return {
      profile,
      settings,
      keeper: this.keeper.state,
      node: this.node.status,
      signer: this.signer.status(),
      vaults,
      defaultVault,
    };
  }

  /** Creates the profile: the sealed signer, the keeper's master key, and the member's own Aztec account. */
  async createProfile(params: ProfileCreateParams): Promise<Status> {
    require(!this.state.file.profile, "this profile already exists");
    require(params.name.trim().length >= 1 &&
      params.name.length <= 48, "a name is 1 to 48 characters");
    require(params.passphrase.length >= 8, "use a passphrase of at least 8 characters");
    const sealing = Buffer.from((await this.signer.create(params.passphrase)).slice(2), "hex");
    const key = await (this.#keys as MasterKeyStore).create(sealing);
    const account: Account = {
      secret: Fr.random().toString(),
      salt: Fr.random().toString(),
      signingKey: GrumpkinScalar.random().toString(),
    };
    await new SecretsStore(this.paths.secrets, key).put(
      ACCOUNT_SECRET,
      Buffer.from(JSON.stringify(account)),
    );
    await this.state.update((file) => {
      file.settings.nodeUrl = params.nodeUrl;
      file.profile = {
        name: params.name.trim(),
        signerKey: this.signer.publicKey(),
        account: "0x00",
        createdAt: Math.floor(Date.now() / 1000),
      };
    });
    await this.#startCore(key);
    return this.status();
  }

  /** Unlocks the signer; with a file-sealed keeper key, the first unlock also starts the keeper and the wallet. */
  async unlock(secret: Secret, ttlSecs: number): Promise<void> {
    require(ttlSecs >= 1 &&
      ttlSecs <=
        this.state.file.settings.unlockTimeoutSecs, "unlock for at most the configured timeout");
    const sealing = this.signer.unlock(secret, ttlSecs);
    if (!this.#core) {
      const key = await (this.#keys as MasterKeyStore).read(Buffer.from(sealing.slice(2), "hex"));
      if (key) await this.#startCore(key);
    }
  }

  async #startCore(key: Buffer): Promise<void> {
    this.#core ??= this.#bootCore(key);
    await this.#core;
  }

  async #bootCore(key: Buffer): Promise<void> {
    this.#secrets = new SecretsStore(this.paths.secrets, key);
    const { settings } = this.state.file;
    await this.keeper.start(
      {
        dataDir: this.paths.keeperDir,
        socket: this.paths.keeperSocket,
        relays: settings.relays,
        log: this.paths.keeperLog,
        ...(this.#options.keeperListen ? { listen: this.#options.keeperListen } : {}),
      },
      key,
    );
    const wallet = await OmeWallet.create(this.node.node, {
      pxe: { dataDirectory: this.paths.pxe, proverEnabled: settings.proverEnabled },
    });
    this.#wallet = wallet;
    const fpc = await getContractInstanceFromInstantiationParams(SponsoredFPCContract.artifact, {
      salt: new Fr(0n),
    });
    await wallet.registerContract(fpc, SponsoredFPCContract.artifact);
    this.#feePayer = fpc.address;
    const stored = await this.secrets().get(ACCOUNT_SECRET);
    if (!stored) throw new DaemonError("failed", "the member's account secrets are missing");
    const account = JSON.parse(stored.toString("utf8")) as Account;
    this.#account = await wallet.addSchnorrAccount(
      Fr.fromString(account.secret),
      Fr.fromString(account.salt),
      GrumpkinScalar.fromString(account.signingKey),
    );
    if (this.state.file.profile?.account !== this.#account.toString()) {
      const address = this.#account.toString() as Hex32;
      await this.state.update((file) => {
        if (file.profile) file.profile.account = address;
      });
    }
    await this.keeper.client.subscribe((event) => {
      this.emit({ type: "keeperEvent", event });
      if ("vault" in event) this.emit({ type: "vaultActivity", vault: event.vault });
    });
    await this.vaults.load();
    await this.membership.load();
    this.#sync = setInterval(() => void this.vaults.sync(), SYNC_INTERVAL_MS);
    this.#sync.unref();
  }

  #keeperChanged(state: KeeperState): void {
    this.emit({ type: "keeper", keeper: state });
  }

  /** One daemon per profile: a lock file naming a live process refuses a second. */
  async #lock(): Promise<void> {
    try {
      const pid = Number((await readFile(this.paths.lock, "utf8")).trim());
      if (pid && pid !== process.pid) {
        process.kill(pid, 0);
        throw new DaemonError(
          "invalid",
          `a daemon already runs for profile ${this.paths.name} (pid ${pid})`,
        );
      }
    } catch (error) {
      if (error instanceof DaemonError) throw error;
    }
    await writePrivate(this.paths.lock, `${process.pid}\n`);
  }
}
