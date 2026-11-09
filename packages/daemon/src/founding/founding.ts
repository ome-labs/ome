import { Fr } from "@aztec-labs/foundation/curves/bn254";
import type {
  DkgResult,
  FoundingState,
  FoundingStep,
  FoundingStepId,
  Hex16,
  InvitePreview,
  KeeperContact,
  Lobby,
  LobbyCreateParams,
  LobbyView,
  StartLobbyParams,
} from "@ome-labs/api";
import {
  decodeVaultProtocolSecrets,
  deriveVault,
  encodeVaultProtocolSecrets,
  fromApiPoint,
  generateVaultProtocolSecrets,
  vaultManifest,
} from "@ome-labs/wallet";

import { type Context, until } from "../context.ts";
import { DaemonError } from "../errors.ts";
import { peerId } from "../peer-id.ts";
import type { VaultRecord } from "../vaults/records.ts";

const STEPS: FoundingStepId[] = ["agree", "dkg", "derive", "register", "deploy", "verify"];

const hex = (bytes: Uint8Array): string => `0x${Buffer.from(bytes).toString("hex")}`;
const contact = (m: {
  identity: string;
  transport: string;
  addresses: string[];
}): KeeperContact => ({
  identity: m.identity,
  transport: m.transport,
  addresses: m.addresses,
});

/**
 * Founding a vault: lobbies through the member's keeper, then, once the host fixed the plan, the DKG, the vault's
 * derivation (every member must derive the same address), registration with the keeper, deployment by the host, and
 * the on-chain config checked by the keeper.
 */
export class FoundingService {
  readonly #context: Context;
  readonly #states = new Map<Hex16, FoundingState>();

  constructor(context: Context) {
    this.#context = context;
  }

  view(lobby: Lobby): LobbyView {
    const me = this.#context.keeper.state.identity;
    return {
      lobby: lobby.lobby,
      role: lobby.role,
      status: lobby.status,
      name: lobby.name,
      threshold: lobby.threshold,
      viewingThreshold: lobby.viewingThreshold,
      network: lobby.network,
      members: lobby.members.map((m) => ({
        index: m.index,
        name: m.name,
        signerKey: m.signerKey,
        identity: m.identity,
        peerId: peerId(m.identity),
        addresses: m.addresses,
        you: m.identity === me,
      })),
      invite: lobby.invite,
      session: lobby.plan?.session ?? null,
    };
  }

  async inspect(invite: string): Promise<InvitePreview> {
    const preview = await this.#context.keeper.client.call("lobby.inspect", { invite });
    return {
      ...preview,
      sameNetwork: preview.network === this.#context.node.network(),
      host: {
        identity: preview.host.identity,
        peerId: peerId(preview.host.identity),
        addresses: preview.host.addresses,
      },
    };
  }

  #member() {
    const profile = this.#context.state.file.profile;
    if (!profile) throw new DaemonError("not-ready", "create a profile first");
    return { name: profile.name, signerKey: profile.signerKey };
  }

  async create(params: LobbyCreateParams): Promise<LobbyView> {
    const keeper = this.#context.keeper.client;
    const { lobby } = await keeper.call("lobby.open", {
      ...params,
      network: this.#context.node.network(),
      member: this.#member(),
    });
    return this.get(lobby);
  }

  async join(invite: string): Promise<LobbyView> {
    const preview = await this.inspect(invite);
    if (!preview.sameNetwork)
      throw new DaemonError(
        "invalid",
        `the invite is for ${preview.network}, this daemon is on ${this.#context.node.network()}`,
      );
    const { lobby } = await this.#context.keeper.client.call("lobby.join", {
      invite,
      member: this.#member(),
    });
    return this.get(lobby);
  }

  async get(lobby: Hex16): Promise<LobbyView> {
    return this.view(await this.#context.keeper.client.call("lobby.status", { lobby }));
  }

  async list(): Promise<LobbyView[]> {
    return (await this.#context.keeper.client.call("lobby.list", {})).map((l) => this.view(l));
  }

  /** Fixes the plan (host): final thresholds, a fresh salt, and fresh protocol secrets the keeper seals to each member. */
  async start(params: StartLobbyParams): Promise<LobbyView> {
    const secrets = encodeVaultProtocolSecrets(await generateVaultProtocolSecrets());
    const lobby = await this.#context.keeper.client.call("lobby.start", {
      ...params,
      salt: Fr.random().toString(),
      secrets: hex(secrets),
    });
    return this.view(lobby);
  }

  async leave(lobby: Hex16): Promise<void> {
    const keeper = this.#context.keeper.client;
    const status = await keeper.call("lobby.status", { lobby });
    await keeper.call(status.role === "host" ? "lobby.close" : "lobby.leave", { lobby });
  }

  state(lobby: Hex16): FoundingState | null {
    return this.#states.get(lobby) ?? null;
  }

  /** Runs the founding of a started lobby in the background; progress arrives as `founding` events. */
  begin(lobby: Hex16): FoundingState {
    const existing = this.#states.get(lobby);
    if (existing && !existing.done) return existing;
    const state: FoundingState = {
      lobby,
      steps: STEPS.map((id) => ({
        id,
        status: "pending",
        startedAt: null,
        finishedAt: null,
        detail: null,
      })),
      vault: null,
      error: null,
      done: false,
    };
    this.#states.set(lobby, state);
    void this.#run(state);
    return state;
  }

  async #run(state: FoundingState): Promise<void> {
    let current: FoundingStepId = "agree";
    const step = async <T>(
      id: FoundingStepId,
      run: () => Promise<T>,
      detail?: (value: T) => string | null,
    ) => {
      current = id;
      const entry = state.steps.find((s) => s.id === id) as FoundingStep;
      entry.status = "active";
      entry.startedAt = Math.floor(Date.now() / 1000);
      this.#context.emit({ type: "founding", founding: state });
      const value = await run();
      entry.status = "done";
      entry.finishedAt = Math.floor(Date.now() / 1000);
      entry.detail = detail?.(value) ?? null;
      this.#context.emit({ type: "founding", founding: state });
      return value;
    };
    try {
      const keeper = () => this.#context.keeper.client;
      const lobby = await step("agree", () =>
        until(async () => {
          const l = await keeper().call("lobby.status", { lobby: state.lobby });
          if (l.status === "closed") throw new DaemonError("invalid", "the lobby was closed");
          return l.plan ? l : undefined;
        }, "the host to start the lobby"),
      );
      const plan = lobby.plan;
      if (!plan?.secrets)
        throw new DaemonError("failed", "the plan carries no protocol secrets for this member");
      const contacts = lobby.members.map(contact);

      const dkg = await step(
        "dkg",
        async () => {
          await keeper()
            .call("ceremony.dkg", {
              session: plan.session,
              threshold: plan.viewingThreshold,
              members: contacts,
            })
            .catch((error: { message?: string }) => {
              if (!error.message?.includes("exists")) throw error;
            });
          return until(async () => {
            const c = await keeper().call("ceremony.status", { session: plan.session });
            if (c.status === "aborted")
              throw new DaemonError("failed", `the DKG aborted: ${c.failure?.reason}`);
            return c.status === "completed" ? (c.result as DkgResult) : undefined;
          }, "the DKG");
        },
        (r) => `transcript ${r.transcript.slice(0, 18)}…`,
      );

      const secrets = Buffer.from(plan.secrets.slice(2), "hex");
      const deployment = await step(
        "derive",
        () =>
          deriveVault({
            signerKeys: lobby.members.map((m) => fromApiPoint(m.signerKey)),
            threshold: plan.threshold,
            viewingThreshold: plan.viewingThreshold,
            groupKey: fromApiPoint(dkg.groupKey),
            verificationShares: dkg.verificationShares.map(fromApiPoint),
            secrets: decodeVaultProtocolSecrets(secrets),
            salt: Fr.fromString(plan.salt),
          }),
        (d) => d.address.toString(),
      );
      const address = deployment.address.toString();
      state.vault = address;

      await step("register", async () => {
        const manifest = vaultManifest(deployment, contacts);
        await keeper().call("vault.register", { session: plan.session, manifest });
        const record: VaultRecord = {
          address,
          name: lobby.name,
          foundedAt: Math.floor(Date.now() / 1000),
          founding: { lobby: state.lobby, session: plan.session, deployTx: null },
          deployment: {
            salt: plan.salt,
            signerKeys: lobby.members.map((m) => m.signerKey),
            threshold: plan.threshold,
            viewingThreshold: plan.viewingThreshold,
            groupKey: dkg.groupKey,
            verificationShares: dkg.verificationShares,
          },
          names: Object.fromEntries(lobby.members.map((m) => [m.identity, m.name])),
          transcripts: { "0": dkg.transcript },
          config: null,
          status: "active",
        };
        await this.#context.vaults.add(record, secrets);
      });

      await step(
        "deploy",
        async () => {
          const vault = this.#context.vaults.address(address);
          if (lobby.role === "host") {
            // Deploying reads the vault's keys by threshold ECDH, which needs co-founders whose keepers have
            // registered the committee too; one that starts a moment later is waited for, not failed.
            const receipt = await until(
              () =>
                this.#context
                  .wallet()
                  .deployVault(vault, { from: this.#context.account(), fee: this.#context.fee() })
                  .catch((error: unknown) => {
                    if (error instanceof Error && /members answered/u.test(error.message))
                      return undefined;
                    throw error;
                  }),
              "the co-founders' keepers to answer",
              120_000,
              2000,
            );
            const record = this.#context.vaults.record(address);
            record.founding.deployTx = receipt.txHash.toString();
            await this.#context.vaults.save(record);
            return receipt.txHash.toString();
          }
          await until(
            () =>
              this.#context
                .wallet()
                .getVaultConfig(vault)
                .then(
                  () => true,
                  () => undefined,
                ),
            "the host to deploy the vault",
          );
          return null;
        },
        (tx) => tx,
      );

      await step("verify", async () => {
        const observation = await this.#context.vaults.observe(address);
        if (observation !== "current")
          throw new DaemonError(
            "alarm",
            `the deployed config does not match the plan (${observation})`,
          );
      });
      state.done = true;
    } catch (error) {
      const entry = state.steps.find((s) => s.status === "active");
      if (entry) entry.status = "failed";
      state.error = {
        step: current,
        message: error instanceof Error ? error.message : String(error),
        blamed: [],
      };
      state.done = true;
    }
    this.#context.emit({ type: "founding", founding: state });
  }
}
