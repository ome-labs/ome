import { randomBytes } from "node:crypto";
import { readFile, rename, rm } from "node:fs/promises";

import { SponsoredFeePaymentMethod } from "@aztec-labs/aztec.js/fee";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { GrumpkinScalar } from "@aztec-labs/foundation/curves/grumpkin";
import { SponsoredFPCContract } from "@aztec-labs/noir-contracts.js/SponsoredFPC";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { getContractInstanceFromInstantiationParams } from "@aztec-labs/stdlib/contract";
import {
  API_VERSION,
  type DaemonEvent,
  type DaemonInfo,
  type FeesStatus,
  type Hex32,
  type Name,
  type KeeperState,
  type ProfileCreateParams,
  type RemoteKeeperParams,
  type Profile,
  type Secret,
  type SignerStatus,
  type Settings,
  type Status,
} from "@ome-labs/api";
import { KeeperClient, OmeWallet } from "@ome-labs/wallet";

import { AppPageServer } from "./apps/page-server.ts";
import { AppService } from "./apps/service.ts";
import { accountAddresses, addressNames, walletDir } from "./aztec/aliases.ts";
import type { Context } from "./context.ts";
import { ContractRegistry } from "./contracts/registry.ts";
import { DeviceService } from "./devices/service.ts";
import { DaemonError, notReady, require } from "./errors.ts";
import { feesStatus } from "./fees.ts";
import { FoundingService } from "./founding/founding.ts";
import { KeeperSupervisor, keeperBinary } from "./keeper/supervisor.ts";
import { CAPABILITIES, LIMITS } from "./limits.ts";
import { MembershipService } from "./membership/service.ts";
import { networkOf, unsupported } from "./node/networks.ts";
import { NodeService } from "./node/node.ts";
import { privateDir, writePrivate } from "./profile/files.ts";
import { type ProfilePaths, omeHome, profilePaths } from "./profile/paths.ts";
import { ProfileState } from "./profile/state.ts";
import { ProposalEvents } from "./proposals/events.ts";
import { countsOf } from "./proposals/filters.ts";
import { ProposalService } from "./proposals/service.ts";
import { type MasterKeyStore, masterKeyStore } from "./secrets/master-key.ts";
import { SecretsStore } from "./secrets/store.ts";
import { handlers } from "./server/handlers.ts";
import { DaemonServer } from "./server/server.ts";
import { SignerService } from "./signer/signer.ts";
import { ReceivedWatcher } from "./vaults/received.ts";
import { VaultService } from "./vaults/service.ts";

/** Release of this daemon. */
export const DAEMON_VERSION = "0.2.0"; // x-release-please-version

const SYNC_INTERVAL_MS = 15_000;
/** Keeper events after which a vault's proposal counts change. */
const RECOUNT: ReadonlySet<string> = new Set([
  "proposal",
  "payload",
  "approval",
  "rejection",
  "receipt",
]);
const ACCOUNT_SECRET = "account";
const REMOTE_KEEPER_TOKEN = "remote-keeper-token";

export interface DaemonOptions {
  readonly profile: string;
  readonly home?: string;
  /** Overrides the profile's settings for this run (tests). */
  readonly settings?: Partial<Settings>;
  /** libp2p listen addresses for the keeper (tests listen on loopback). */
  readonly keeperListen?: readonly string[];
  /** Ends the process once a deleted profile's daemon has stopped (`omed` signals itself; tests do nothing). */
  readonly exit?: () => void;
  /**
   * Serves the app page on 127.0.0.1 at this port (0 picks one), a development and test transport for app
   * connections (`OME_APPS_PORT`). Off when absent.
   */
  readonly appsPort?: number;
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
  readonly contracts: ContractRegistry;
  readonly founding: FoundingService;
  readonly proposals: ProposalService;
  readonly membership: MembershipService;
  readonly devices: DeviceService;
  readonly apps: AppService;
  readonly #proposalEvents: ProposalEvents;
  readonly #received: ReceivedWatcher;
  readonly #options: DaemonOptions;
  readonly #startedAt = Math.floor(Date.now() / 1000);
  #state: ProfileState | undefined;
  #node: NodeService | undefined;
  #keys: MasterKeyStore | undefined;
  #secrets: SecretsStore | undefined;
  #wallet: OmeWallet | undefined;
  #account: AztecAddress | undefined;
  #feePayer: AztecAddress | undefined;
  /** The fee payer's last read status, refreshed in the background: `status` never waits on the node. */
  #fees: FeesStatus | null = null;
  #server: DaemonServer | undefined;
  #appPage: AppPageServer | undefined;
  #sync: NodeJS.Timeout | undefined;
  #core: Promise<void> | undefined;
  #masterKey: Buffer | undefined;
  /** Senders already registered with the PXE, from `aztec-wallet`'s accounts. */
  readonly #senders = new Set<string>();

  constructor(options: DaemonOptions) {
    this.#options = options;
    this.paths = profilePaths(options.profile, options.home ?? omeHome());
    this.signer = new SignerService(this.paths.signer);
    this.keeper = new KeeperSupervisor(
      () => keeperBinary(options.home ?? omeHome()),
      (state) => this.#keeperChanged(state),
    );
    this.vaults = new VaultService(this);
    this.contracts = new ContractRegistry(this);
    this.founding = new FoundingService(this);
    this.proposals = new ProposalService(this);
    this.membership = new MembershipService(this);
    this.devices = new DeviceService(this, this.proposals, this.membership);
    this.apps = new AppService(this, this.proposals);
    this.proposals.rotationApprover = (vault, stored, tx, secret) =>
      this.membership.approve(vault, stored, tx, secret);
    this.#proposalEvents = new ProposalEvents(
      (ref) => this.proposals.get(ref),
      (event) => this.emit(event),
      (ref) => this.proposals.ensureContracts(ref),
      {
        list: async (vault) => {
          const views = await this.proposals.list(vault);
          this.vaults.setCounts(vault, countsOf(views));
          return views;
        },
        member: (vault, index) => this.vaults.memberRef(vault, index),
        now: () => this.wallet().chainTime(),
      },
    );
    this.#received = new ReceivedWatcher(this);
  }

  // Context.

  get state(): ProfileState {
    if (!this.#state) throw notReady("starting", "the daemon is starting");
    return this.#state;
  }

  get node(): NodeService {
    if (!this.#node) throw notReady("starting", "the daemon is starting");
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
    if (!this.#secrets) throw notReady("signer", "unlock the signer first");
    return this.#secrets;
  }

  account(): AztecAddress {
    if (!this.#account) throw notReady("starting", "the member's account is not loaded yet");
    return this.#account;
  }

  feePayer(): AztecAddress {
    if (!this.#feePayer) throw notReady("starting", "the wallet is not running yet");
    return this.#feePayer;
  }

  fee() {
    return { paymentMethod: new SponsoredFeePaymentMethod(this.feePayer()) };
  }

  /**
   * Pays through the `SponsoredFPC` instance with `salt`: zero is the canonical one the local network and Testnet
   * provide; on Alpha a member deploys and funds their own. It is also the fee payer the signer trusts in rotations
   * and that paired devices are told about.
   */
  async setFeePayer(salt: string): Promise<void> {
    const fpc = await getContractInstanceFromInstantiationParams(SponsoredFPCContract.artifact, {
      salt: Fr.fromString(salt),
    });
    await this.wallet().registerContract(fpc, SponsoredFPCContract.artifact);
    this.#feePayer = fpc.address;
  }

  emit(event: DaemonEvent): void {
    this.#server?.broadcast(event);
  }

  names(): Promise<Map<Hex32, string>> {
    return addressNames(walletDir(this.state.file.settings.aztecWalletDir));
  }

  /**
   * Registers `aztec-wallet`'s accounts with the PXE as senders, so their notes to the member's vaults are found by
   * the tags they share with them. Runs at start and with every sync, since aliases change outside the daemon.
   */
  async registerSenders(): Promise<void> {
    if (!this.#wallet) return;
    for (const address of await accountAddresses(
      walletDir(this.state.file.settings.aztecWalletDir),
    )) {
      if (this.#senders.has(address)) continue;
      // oxlint-disable-next-line no-await-in-loop
      await this.#wallet.registerSender(AztecAddress.fromStringUnsafe(address));
      this.#senders.add(address);
    }
  }

  // Lifecycle.

  /** Takes the profile lock, opens the socket and, with a profile whose keeper key is readable, starts the rest. */
  async start(): Promise<void> {
    await privateDir(this.paths.dir);
    await this.#lock();
    this.#state = await ProfileState.load(this.paths.state, this.#options.settings);
    this.#node = new NodeService(this.state.file.settings.nodeUrl);
    await this.node.check();
    // A passphrase change writes both new files before swapping them in: finish one that was interrupted.
    await rename(`${this.paths.masterKey}.next`, this.paths.masterKey).catch(() => undefined);
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
    if (this.#options.appsPort !== undefined)
      this.#appPage = await AppPageServer.listen(this.#options.appsPort, this.apps);
    if (this.state.file.profile) {
      const key = await this.#keys.read();
      if (key) await this.#startCore(key);
    }
  }

  async stop(): Promise<void> {
    clearInterval(this.#sync);
    this.devices.stop();
    await this.#server?.close();
    await this.#appPage?.close();
    await this.#wallet?.stop();
    await this.keeper.stop();
    this.signer.lock();
    await Promise.all(
      [this.paths.addr, this.paths.token, this.paths.lock].map((p) => rm(p, { force: true })),
    );
  }

  /** The app page's origin, when `appsPort` serves it. */
  get appPage(): string | undefined {
    return this.#appPage?.origin;
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
      limits: LIMITS,
      capabilities: [...CAPABILITIES],
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
      fees: this.#fees,
    };
  }

  /** A rename a member's keeper replicated, or this member made: applied, then announced. */
  async #nameChanged(vault: Hex32, name: Name): Promise<void> {
    const event = await this.vaults.applyName(vault, name).catch(() => undefined);
    if (event) this.emit(event);
  }

  /** The vaults this member is active in. */
  #activeVaults(): Hex32[] {
    return this.vaults
      .list()
      .filter((r) => r.status === "active")
      .map((r) => r.address);
  }

  /** Senders registered with the PXE from `aztec-wallet`'s accounts. */
  get senderCount(): number {
    return this.#senders.size;
  }

  /** Recounts `vault`'s open proposals for `status`; a failed read keeps the last counts. */
  async refreshCounts(vault: Hex32): Promise<void> {
    const views = await this.proposals.list(vault).catch(() => undefined);
    if (views) this.vaults.setCounts(vault, countsOf(views));
  }

  /** Reads the fee payer's Fee Juice now and keeps it for `status`. */
  async refreshFees(): Promise<FeesStatus> {
    this.#fees = await feesStatus(this);
    return this.#fees;
  }

  /**
   * Re-seals the signing key under `next`, and the keeper's master key with it when a sealed file holds it. Both new
   * files are written beside the old ones before either is swapped in, and `start` finishes an interrupted swap, so the
   * passphrase never opens one and not the other.
   */
  async changePassphrase(current: Secret, next: string): Promise<SignerStatus> {
    require(next.length >= 8, "use a passphrase of at least 8 characters");
    const keys = this.#keys as MasterKeyStore;
    const resealed = this.signer.resealed(current, next);
    const currentKey = "key" in current ? current.key : this.signer.sealingKey(current.passphrase);
    const master = await keys.resealed(
      Buffer.from(currentKey.slice(2), "hex"),
      Buffer.from(resealed.key.slice(2), "hex"),
    );
    if (master) await writePrivate(`${this.paths.masterKey}.next`, master);
    await writePrivate(`${this.signer.path}.next`, resealed.sealed);
    if (master) await rename(`${this.paths.masterKey}.next`, this.paths.masterKey);
    await rename(`${this.signer.path}.next`, this.signer.path);
    this.signer.replaced(resealed.sealed);
    const status = this.signer.status();
    this.emit({ type: "signer", signer: status });
    this.emit({ type: "sealingKeyChanged" });
    return status;
  }

  /** Renames the member: locally, and for every vault's members through a signed rename each keeper replicates. */
  async renameProfile(name: string): Promise<Profile> {
    require(name.trim().length >= 1 && name.length <= 48, "a name is 1 to 48 characters");
    if (!this.state.file.profile) throw notReady("profile", "create a profile first");
    await this.state.update((file) => {
      if (file.profile) file.profile.name = name;
    });
    for (const vault of this.#activeVaults()) {
      // oxlint-disable-next-line no-await-in-loop
      const renamed = await this.keeper.client.call("names.put", {
        vault,
        subject: "member",
        name,
      });
      // oxlint-disable-next-line no-await-in-loop
      const event = await this.vaults.applyName(vault, renamed);
      if (event) this.emit(event);
    }
    return this.state.file.profile as Profile;
  }

  /**
   * Deletes this profile and stops: the keeper, its store and viewing shares, the signing key and every file.
   * Refused while the profile is in a vault, since the vault would lose a viewing share and a signer; leave or be
   * removed first (a removed member's vault is `removed`).
   */
  deleteProfile(confirm: string): void {
    require(confirm === this.paths.name, `type the profile's name, ${this.paths.name}, to confirm`);
    const vaults = this.vaults.list().filter((r) => r.status !== "removed");
    if (vaults.length > 0)
      throw new DaemonError(
        "invalid",
        `this profile is a member of ${vaults.map((v) => v.name).join(", ")}: leave ${vaults.length === 1 ? "it" : "them"} first (a change of members that removes you), then delete it`,
      );
    setImmediate(() => {
      void (async () => {
        await this.stop();
        await (this.#keys as MasterKeyStore | undefined)?.destroy();
        await rm(this.paths.dir, { recursive: true, force: true });
        (this.#options.exit ?? (() => process.emit("SIGTERM")))();
      })();
    });
  }

  /** Creates the profile: the sealed signer, the keeper's master key, and the member's own Aztec account. */
  async createProfile(params: ProfileCreateParams): Promise<Status> {
    require(!this.state.file.profile, "this profile already exists");
    require(params.name.trim().length >= 1 &&
      params.name.length <= 48, "a name is 1 to 48 characters");
    require(params.passphrase.length >= 8, "use a passphrase of at least 8 characters");
    // The account is derived on the node: refuse before sealing anything rather than leave a half-made profile.
    const node = await this.node.check();
    if (!node.reachable)
      throw new DaemonError(
        "not-ready",
        `no Aztec node answers at ${node.url}: ${node.error ?? "no answer"}`,
      );
    const refused = unsupported(node);
    if (refused) throw new DaemonError("invalid", refused);
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
      // A new profile on Testnet reaches the other members through Ome Labs' relay unless relays were chosen.
      if (file.settings.relays.length === 0)
        file.settings.relays = [...(networkOf(node)?.relays ?? [])];
      file.profile = {
        name: params.name.trim(),
        signerKey: this.signer.publicKey(),
        // Zero until the wallet derives the account as it starts.
        account: `0x${"00".repeat(32)}`,
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
    // A failed start is not kept: the next unlock or profile command tries again.
    this.#core ??= this.#bootCore(key).catch((error: unknown) => {
      this.#core = undefined;
      throw error;
    });
    await this.#core;
  }

  async #bootCore(key: Buffer): Promise<void> {
    // A profile whose node moved to an unsupported network (Alpha) does not start; an unreachable node is checked again
    // later.
    const node = await this.node.check();
    const refused = node.reachable ? unsupported(node) : null;
    if (refused) throw new DaemonError("invalid", refused);
    this.#secrets = new SecretsStore(this.paths.secrets, key);
    this.#masterKey = key;
    const { settings } = this.state.file;
    await this.keeper.subscribe((event) => {
      this.emit({ type: "keeperEvent", event });
      void this.#proposalEvents.onKeeperEvent(event);
      if (RECOUNT.has(event.type) && "vault" in event)
        void this.refreshCounts(event.vault).catch(() => undefined);
      if (event.type === "lobby")
        void this.founding
          .get(event.lobby)
          .then((lobby) => this.emit({ type: "lobby", lobby, change: event.change }))
          .catch(() => undefined);
      if (event.type === "name") void this.#nameChanged(event.vault, event.name);
      if (event.type === "artifact" && event.complete)
        void this.#artifactArrived(event.vault, event.classId);
      this.devices.onKeeperEvent(event);
    });
    await this.#startKeeper();
    const wallet = await OmeWallet.create(this.node.node, {
      pxe: { dataDirectory: this.paths.pxe, proverEnabled: settings.proverEnabled },
    });
    this.#wallet = wallet;
    await this.setFeePayer(settings.feePayerSalt);
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
    await this.registerSenders();
    await this.vaults.load();
    await this.contracts.load();
    await this.membership.load();
    void this.refreshFees().catch(() => undefined);
    this.#sync = setInterval(() => {
      void this.vaults.sync().then(() => this.#checkReceived());
      void this.registerSenders();
      void this.refreshFees().catch(() => undefined);
      void this.#proposalEvents.tick(this.#activeVaults()).catch(() => undefined);
    }, SYNC_INTERVAL_MS);
    this.#sync.unref();
  }

  /** A contract's code a member's keeper fetched: checked and registered, then announced so reviews refresh. */
  async #artifactArrived(vault: Hex32, classId: Hex32): Promise<void> {
    if (await this.contracts.fetched(vault, classId).catch(() => false))
      this.emit({ type: "contractAvailable", vault, classId });
  }

  /** Announces transfers into each active vault that arrived since the last sync. */
  async #checkReceived(): Promise<void> {
    for (const record of this.vaults.list().filter((r) => r.status === "active")) {
      // One vault at a time: each read goes through the keepers' threshold ECDH.
      // oxlint-disable-next-line no-await-in-loop
      await this.#received.check(record.address);
    }
  }

  /** Starts the keeper the settings name: a remote one, or the daemon's own. */
  async #startKeeper(): Promise<void> {
    const { settings } = this.state.file;
    const remote = settings.remoteKeeper;
    if (remote) {
      const token = await this.secrets().get(REMOTE_KEEPER_TOKEN);
      if (!token) throw new DaemonError("failed", "the remote keeper's token is missing");
      await this.keeper.startRemote({ ...remote, token: token.toString("utf8") });
      return;
    }
    if (!this.#masterKey) throw notReady("signer", "unlock the signer first");
    await this.keeper.start(
      {
        dataDir: this.paths.keeperDir,
        socket: this.paths.keeperSocket,
        relays: settings.relays,
        log: this.paths.keeperLog,
        ...(this.#options.keeperListen ? { listen: this.#options.keeperListen } : {}),
      },
      this.#masterKey,
    );
  }

  /**
   * Uses a keeper on another machine, after checking that it answers with that fingerprint and token, or the daemon's
   * own again. Vaults stay with the keeper that holds their shares: switch before founding or joining them.
   */
  async connectKeeper(remote: RemoteKeeperParams | null): Promise<KeeperState> {
    if (remote) {
      const probe = await KeeperClient.connectRemote(
        remote.address,
        remote.fingerprint,
        remote.token,
      );
      probe.close();
      await this.secrets().put(REMOTE_KEEPER_TOKEN, Buffer.from(remote.token, "utf8"));
    }
    await this.state.update((file) => {
      file.settings.remoteKeeper = remote
        ? { address: remote.address, fingerprint: remote.fingerprint.toLowerCase() }
        : null;
    });
    if (!remote) await this.secrets().delete(REMOTE_KEEPER_TOKEN);
    await this.#startKeeper();
    if (this.keeper.state.status === "running") await this.vaults.load();
    return this.keeper.state;
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
