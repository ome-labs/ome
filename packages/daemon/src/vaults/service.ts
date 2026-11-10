import { join } from "node:path";

import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type {
  AuditView,
  CommitteeView,
  DaemonEvent,
  Hex32,
  Manifest,
  MemberReachability,
  MemberRef,
  Name,
  ProposalCounts,
  VaultConfig,
  VaultDetails,
  VaultSummary,
} from "@ome-labs/api";
import {
  KeeperPartialsSource,
  type OmeWallet,
  type VaultDeployment,
  decodeVaultProtocolSecrets,
  deriveVault,
} from "@ome-labs/wallet";
import { decodeProposal, signerSetCommitment } from "@ome-labs/wasm";

import type { Context } from "../context.ts";
import { DaemonError, notReady } from "../errors.ts";
import { peerId } from "../peer-id.ts";
import { readAllVersioned, writeVersioned } from "../profile/files.ts";
import type { Schema } from "../profile/schema.ts";
import { type VaultRecord, secretsName, setupOf } from "./records.ts";

/** The member's vaults: their records, their current committees, and their place in the wallet. */
/** A vault record's migrations (`vaults/<address>.json`). */
export const VAULT_RECORD_SCHEMA: Schema = {
  name: "vault record",
  migrations: [
    // 1: the file gains `schema`.
    (file) => file,
  ],
};

export class VaultService {
  readonly #context: Context;
  readonly #records = new Map<Hex32, VaultRecord>();
  readonly #manifests = new Map<Hex32, Manifest>();
  readonly #deployments = new Map<Hex32, VaultDeployment>();

  constructor(context: Context) {
    this.#context = context;
  }

  async load(): Promise<void> {
    for (const record of await readAllVersioned<VaultRecord>(
      this.#context.paths.vaults,
      VAULT_RECORD_SCHEMA,
    )) {
      this.#records.set(record.address, record);
      // oxlint-disable-next-line no-await-in-loop
      if (record.status === "active") await this.attach(record.address);
    }
  }

  list(): VaultRecord[] {
    return [...this.#records.values()];
  }

  record(vault: Hex32): VaultRecord {
    const record = this.#records.get(vault.toLowerCase());
    if (!record) throw new DaemonError("not-found", `unknown vault ${vault}`);
    return record;
  }

  manifest(vault: Hex32): Manifest {
    const manifest = this.#manifests.get(this.record(vault).address);
    if (!manifest) throw notReady("vault", "the vault's committee is not loaded yet");
    return manifest;
  }

  deployment(vault: Hex32): VaultDeployment {
    const deployment = this.#deployments.get(this.record(vault).address);
    if (!deployment) throw notReady("vault", "the vault is not loaded yet");
    return deployment;
  }

  address(vault: Hex32): AztecAddress {
    return AztecAddress.fromStringUnsafe(this.record(vault).address);
  }

  /** This member's index in the vault's current committee. */
  myIndex(vault: Hex32): number {
    const me = this.#context.keeper.state.identity;
    const member = this.manifest(vault).members.find((m) => m.identity === me);
    if (!member) throw new DaemonError("invalid", "this member is not in the vault's committee");
    return member.index;
  }

  /** Stores a vault and its sealed protocol secrets, then loads it. */
  async add(record: VaultRecord, secrets: Uint8Array): Promise<void> {
    await this.#context.secrets().put(secretsName(record.address), secrets);
    await this.save(record);
    await this.attach(record.address);
  }

  async save(record: VaultRecord): Promise<void> {
    this.#records.set(record.address, record);
    await writeVersioned(
      join(this.#context.paths.vaults, `${record.address}.json`),
      VAULT_RECORD_SCHEMA,
      record,
    );
  }

  /**
   * Registers the vault in the wallet (once) and points its viewing at the committee the keeper serves now, with that
   * epoch's tagging secret.
   */
  async attach(vault: Hex32): Promise<void> {
    const record = this.record(vault);
    const wallet = this.#context.wallet();
    const keeper = this.#context.keeper.client;
    const { manifest } = await keeper.call("vault.status", { vault: record.address });
    const transcript = record.transcripts[String(manifest.epoch)];
    if (!transcript) throw new DaemonError("failed", `no transcript for epoch ${manifest.epoch}`);
    const viewing = { manifest, transcript, source: new KeeperPartialsSource(keeper) };
    this.#manifests.set(record.address, manifest);
    if (this.#deployments.has(record.address)) {
      await wallet.setViewing(this.address(vault), viewing);
      return;
    }
    const sealed = await this.#context.secrets().get(secretsName(record.address));
    if (!sealed) throw new DaemonError("failed", "the vault's protocol secrets are missing");
    const deployment = await deriveVault(setupOf(record, decodeVaultProtocolSecrets(sealed)));
    if (deployment.address.toString() !== record.address)
      throw new DaemonError("failed", "the recorded deployment derives another address");
    await wallet.registerVault(deployment, viewing);
    this.#deployments.set(record.address, deployment);
    // Earlier epochs' self-sends (a joiner received their transcripts in the reshare).
    for (const [epoch, earlier] of Object.entries(record.transcripts)) {
      if (Number(epoch) === manifest.epoch) continue;
      // oxlint-disable-next-line no-await-in-loop
      await wallet.addEpochSecret(this.address(vault), Number(epoch), earlier);
    }
  }

  /**
   * Reads each vault's config note and hands it to the keeper (`vault.observeConfig`). An activated committee
   * re-points viewing at it; a removal or an unverifiable config is reported by the keeper's own events.
   */
  async sync(): Promise<void> {
    for (const record of this.list().filter(
      (r) => r.status === "active" && this.#deployments.has(r.address),
    )) {
      // oxlint-disable-next-line no-await-in-loop
      await this.observe(record.address).catch(() => undefined);
      // oxlint-disable-next-line no-await-in-loop
      await this.syncNames(record.address).catch(() => undefined);
    }
  }

  /** Applies the current names the keeper holds: renames made while this daemon was down. */
  async syncNames(vault: Hex32): Promise<void> {
    const names = await this.#context.keeper.client.call("names.list", { vault });
    for (const name of names) {
      // oxlint-disable-next-line no-await-in-loop
      const event = await this.applyName(vault, name);
      if (event) this.#context.emit(event);
    }
  }

  async observe(vault: Hex32): Promise<string> {
    const record = this.record(vault);
    const wallet: OmeWallet = this.#context.wallet();
    const config = await wallet.getVaultConfig(this.address(vault));
    const { observation } = await this.#context.keeper.client.call("vault.observeConfig", {
      vault: record.address,
      config,
    });
    record.config = {
      config,
      readAt: Math.floor(Date.now() / 1000),
      matchesCommittee: observation !== "unknown",
    };
    if (observation === "activated") await this.attach(vault);
    if (observation === "removed") record.status = "removed";
    const swapped = observation === "current" && (await this.#adoptSwap(record.address, config));
    await this.save(record);
    if (swapped || (observation !== "current" && observation !== "outdated")) {
      // A swap or threshold change, an activated or unknown committee, or a removal: the config and who is in the
      // vault, or how many must approve, changed.
      this.#context.emit({ type: "configChanged", vault: record.address, version: config.version });
      this.#context.emit({ type: "memberChanged", vault: record.address });
    }
    if (observation === "removed")
      this.#context.emit({ type: "vaultUpdated", vault: record.address });
    return observation;
  }

  /**
   * Makes sure this member acts on the vault's current policy: when the config on chain names other signers or another
   * threshold than the manifest (a swap or threshold change just ran), the config is observed now, so the keeper adopts
   * the change, instead of at the next sync.
   */
  async ensureCurrent(vault: Hex32): Promise<void> {
    const manifest = this.#manifests.get(vault);
    if (!manifest) return;
    const config = await this.#context.wallet().getVaultConfig(this.address(vault));
    if (
      manifest.threshold !== config.threshold ||
      signerSetCommitment(manifest.members.map((m) => m.signerKey)) !== config.signerSetCommitment
    )
      await this.observe(vault);
  }

  /**
   * After a signer swap or a threshold change ran, the config's signer set or threshold no longer matches the
   * committee's manifest: the keeper adopts the change from the executed proposal (`vault.updateSigners`, which checks
   * it against this config), and viewing is re-pointed at the updated manifest. Returns whether a change was adopted.
   */
  async #adoptSwap(vault: Hex32, config: VaultConfig): Promise<boolean> {
    const manifest = this.#manifests.get(vault);
    if (!manifest) return false;
    if (
      signerSetCommitment(manifest.members.map((m) => m.signerKey)) ===
        config.signerSetCommitment &&
      manifest.threshold === config.threshold
    )
      return false;
    const keeper = this.#context.keeper.client;
    const bodies = await keeper.call("proposals.list", { vault });
    for (const body of bodies.toReversed()) {
      const proposal = decodeProposal(body).id;
      // oxlint-disable-next-line no-await-in-loop
      const adopted = await keeper.call("vault.updateSigners", { vault, proposal }).then(
        () => true,
        () => false,
      );
      if (adopted) {
        // oxlint-disable-next-line no-await-in-loop
        await this.attach(vault);
        return true;
      }
    }
    return false;
  }

  /** Each vault's open proposals at a glance, refreshed in the background for `summary`. */
  readonly #counts = new Map<Hex32, ProposalCounts>();

  setCounts(vault: Hex32, counts: ProposalCounts): void {
    this.#counts.set(vault, counts);
  }

  /** Members' accounts with their names: `account → name`, for labelling a recipient as a member. */
  memberAccounts(vault: Hex32): Map<Hex32, string> {
    const record = this.record(vault);
    const manifest = this.#manifests.get(record.address);
    const out = new Map<Hex32, string>();
    for (const m of manifest?.members ?? []) {
      const account = record.accounts?.[m.identity];
      if (account) out.set(account, record.names[m.identity] ?? `member ${m.index}`);
    }
    return out;
  }

  /**
   * Applies a rename the keeper reports: the vault's name, or a member's. Returns the event to announce, or nothing
   * when it changes nothing here.
   */
  async applyName(vault: Hex32, name: Name): Promise<DaemonEvent | undefined> {
    const record = this.record(vault);
    if (name.subject === "vault") {
      if (record.name === name.name) return undefined;
      record.name = name.name;
      await this.save(record);
      return {
        type: "vaultRenamed",
        vault: record.address,
        name: name.name,
        by:
          name.member === null
            ? { index: 0, name: "a former member" }
            : this.memberRef(vault, name.member),
      };
    }
    if (record.names[name.by] === name.name) return undefined;
    record.names = { ...record.names, [name.by]: name.name };
    await this.save(record);
    if (name.member === null) return undefined;
    return {
      type: "memberRenamed",
      vault: record.address,
      member: this.memberRef(vault, name.member),
    };
  }

  /** Member `index` of `vault`, named as this member knows them. */
  memberRef(vault: Hex32, index: number): MemberRef {
    const record = this.record(vault);
    const member = this.#manifests.get(record.address)?.members.find((m) => m.index === index);
    return {
      index,
      name: (member && record.names[member.identity]) ?? `member ${index}`,
    };
  }

  summary(vault: Hex32): VaultSummary {
    const record = this.record(vault);
    const { aliases, defaultVault } = this.#context.state.file;
    const manifest = this.#manifests.get(record.address);
    return {
      address: record.address,
      name: record.name,
      alias: aliases[record.address] ?? null,
      isDefault: defaultVault === record.address,
      threshold: manifest?.threshold ?? record.deployment.threshold,
      viewingThreshold: manifest?.viewingThreshold ?? record.deployment.viewingThreshold,
      members: manifest?.members.length ?? record.deployment.signerKeys.length,
      myIndex: manifest ? this.myIndex(vault) : 0,
      foundedAt: record.foundedAt,
      counts: this.#counts.get(record.address) ?? { open: 0, ready: 0, needsMe: 0 },
    };
  }

  async details(vault: Hex32): Promise<VaultDetails> {
    const record = this.record(vault);
    const deployment = this.deployment(vault);
    const manifest = this.manifest(vault);
    const [status, protocol] = await Promise.all([
      this.#context.keeper.client.call("vault.status", { vault: record.address }),
      this.#context.keeper.client.call("vault.protocol", { vault: record.address }),
    ]);
    const me = this.#context.keeper.state.identity;
    const releaseOf = (index: number) =>
      protocol.members.find((m) => m.member === index)?.release ?? null;
    const committee: CommitteeView = {
      epoch: manifest.epoch,
      viewingThreshold: manifest.viewingThreshold,
      groupKey: manifest.groupKey,
      members: manifest.members.map((m) => ({
        index: m.index,
        name: record.names[m.identity] ?? `member ${m.index}`,
        account: record.accounts?.[m.identity] ?? null,
        signerKey: m.signerKey,
        verificationShare: m.verificationShare,
        identity: m.identity,
        transport: m.transport,
        peerId: peerId(m.identity),
        addresses: m.addresses,
        release: releaseOf(m.index),
        you: m.identity === me,
      })),
      protocolVersion: protocol.version,
      serving: status.serving,
      keeperStatus: status.status,
    };
    return {
      summary: this.summary(vault),
      contract: {
        address: record.address,
        preaddress: deployment.preaddress.toString(),
        partialAddress: deployment.partialAddress.toString(),
        classId: deployment.instance.currentContractClassId.toString(),
        salt: record.deployment.salt,
        deployer: deployment.instance.deployer.toString(),
        initializationHash: deployment.instance.initializationHash.toString(),
        // Only the incoming viewing key is published as a point; the others are known by their hashes.
        publicKeys: [{ name: "ivpk", key: record.deployment.groupKey }],
      },
      config: record.config
        ? {
            ...record.config.config,
            matchesCommittee: record.config.matchesCommittee,
            readAt: record.config.readAt,
          }
        : null,
      committee,
      founding: {
        session: record.founding.session,
        transcript: record.transcripts["0"] ?? null,
        deployTx: record.founding.deployTx,
      },
    };
  }

  async peers(vault: Hex32): Promise<MemberReachability[]> {
    const record = this.record(vault);
    const peers = await this.#context.keeper.client.call("peers.status", { vault: record.address });
    return peers.map((p) =>
      Object.assign(p, {
        name: record.names[p.identity] ?? `member ${p.index}`,
        account: record.accounts?.[p.identity] ?? null,
      }),
    );
  }

  async audit(vault: Hex32, limit: number): Promise<AuditView[]> {
    const record = this.record(vault);
    const entries = await this.#context.keeper.client.call("audit.list", {
      vault: record.address,
      limit,
    });
    return entries.map((e) =>
      Object.assign(e, { requesterName: record.names[e.requester] ?? null }),
    );
  }
}
