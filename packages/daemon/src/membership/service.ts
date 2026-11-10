import { randomBytes } from "node:crypto";
import { join } from "node:path";

import { Contract } from "@aztec-labs/aztec.js/contracts";
import { poseidon2HashWithSeparator } from "@aztec-labs/foundation/crypto/poseidon";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { siloNullifier } from "@aztec-labs/stdlib/hash";
import { MerkleTreeId } from "@aztec-labs/stdlib/trees";
import { mergeExecutionPayloads } from "@aztec-labs/stdlib/tx";
import type {
  Hex32,
  Joiner,
  Manifest,
  MembershipParams,
  MembershipPlan,
  NewMember,
  ReshareResult,
  RotationState,
  Secret,
  RotationStep,
  RotationStepId,
  SwapSignerParams,
  VaultConfig,
} from "@ome-labs/api";
import { fromApiPoint, vaultArtifact, type VaultTx } from "@ome-labs/wallet";
import {
  type Rotation,
  checkRotation,
  committeePolynomial,
  decodeJoinRequest,
  decodeProposal,
} from "@ome-labs/wasm";

import { type Context, until } from "../context.ts";
import { DaemonError, notReady, require } from "../errors.ts";
import { readAllVersioned, writeVersioned } from "../profile/files.ts";
import type { Schema } from "../profile/schema.ts";
import { summaryOf } from "../proposals/intents.ts";
import type { Stored } from "../proposals/views.ts";
import { decodeHandoff, encodeHandoff } from "../secrets/handoff.ts";
import { counterparties } from "../vaults/activity.ts";
import type { DeploymentRecord, VaultRecord } from "../vaults/records.ts";
import { secretsName } from "../vaults/records.ts";

/** Prefix of a rotation plan's string form. */
const PLAN_PREFIX = "ome-rotation-v0:";
/** `compute_tx_nullifier`'s separator in aztec-nr 6.0.0 (`authwit/account.nr`). */
const DOM_SEP_TX_NULLIFIER = 1025801951;
const STEPS: RotationStepId[] = ["reshare", "propose", "approve", "execute", "activate"];

/** What every participant needs to start the same reshare and recognise the same proposal. */
interface PlanFile {
  readonly v: 0;
  readonly vault: Hex32;
  readonly name: string;
  readonly session: Hex32;
  /** The rotation proposal's entrypoint nonce: its nullifier on chain tells a joiner the rotation ran. */
  readonly txNonce: Hex32;
  readonly planner: number;
  readonly remove: number[];
  readonly dealers: number[];
  readonly threshold: number;
  readonly viewingThreshold: number;
  readonly newMembers: NewMember[];
  readonly names: Record<Hex32, string>;
  /** Members' own Aztec accounts by keeper identity (absent in plans from older daemons). */
  readonly accounts?: Record<Hex32, Hex32>;
  /** The committee and config the rotation starts from. */
  readonly current: Manifest;
  readonly config: VaultConfig;
  /** The founding constructor, so a joiner derives the vault's address. */
  readonly deployment: DeploymentRecord;
}

/** A rotation as this daemon takes part in it. */
interface Rotating {
  state: RotationState;
  readonly plan: PlanFile;
  role: "planner" | "dealer" | "stays" | "joins";
  result: ReshareResult | null;
}

const encodePlan = (plan: PlanFile): string =>
  `${PLAN_PREFIX}${Buffer.from(JSON.stringify(plan), "utf8").toString("base64url")}`;

function decodePlan(text: string): PlanFile {
  const body = text.trim();
  const newer = /^ome-rotation-v([0-9]+):/u.exec(body)?.[1];
  if (newer !== undefined && newer !== "0")
    throw new DaemonError(
      "invalid",
      `this rotation plan needs a newer release of Ome (format v${newer})`,
    );
  require(body.startsWith(PLAN_PREFIX), "not an Ome rotation plan");
  try {
    const plan = JSON.parse(
      Buffer.from(body.slice(PLAN_PREFIX.length), "base64url").toString("utf8"),
    ) as PlanFile;
    require(plan.v === 0 && Array.isArray(plan.newMembers), "not a v0 rotation plan");
    return plan;
  } catch (error) {
    if (error instanceof DaemonError) throw error;
    throw new DaemonError("invalid", "the rotation plan does not decode");
  }
}

const pad = (points: readonly { x: string; y: string }[]) =>
  Array.from({ length: 8 }, (_, i) => {
    const point = points[i];
    return point ? fromApiPoint(point) : { x: Fr.ZERO, y: Fr.ZERO };
  });

/**
 * Membership changes end to end. A member plans the change and hands the plan string to everyone who stays and every
 * joiner; each passes it to `membership.begin`. Then:
 *
 * 1. the reshare: dealers (members who stay) deal to the new committee, handing joiners the protocol secrets;
 * 2. the planner proposes `rotate_membership` under the plan's nonce;
 * 3. members approve it through `proposals.approve`, which compares it with their own keeper's reshare result;
 * 4. any member executes it;
 * 5. keepers activate the new committee: members read the new config note; a joiner, which cannot read it yet, sees
 *    the plan nonce's nullifier on chain and hands its keeper the config the rotation sets.
 */
/** A rotation in progress's migrations (`rotations/<session>.json`). */
export const ROTATION_SCHEMA: Schema = {
  name: "rotation",
  migrations: [
    // 1: the file gains `schema`.
    (file) => file,
  ],
};

export class MembershipService {
  readonly #context: Context;
  readonly #rotations = new Map<Hex32, Rotating>();

  constructor(context: Context) {
    this.#context = context;
  }

  async load(): Promise<void> {
    for (const rotating of await readAllVersioned<Rotating>(
      this.#context.paths.rotations,
      ROTATION_SCHEMA,
    )) {
      this.#rotations.set(rotating.state.plan.id, rotating);
    }
  }

  async request(): Promise<{ request: string }> {
    const profile = this.#context.state.file.profile;
    if (!profile) throw notReady("profile", "create a profile first");
    return this.#context.keeper.client.call("keeper.joinRequest", {
      name: profile.name,
      account: profile.account,
      signerKey: profile.signerKey,
    });
  }

  /** Checks a membership change against the vault and the contract's rules, and returns the plan to share. */
  async plan(params: MembershipParams): Promise<MembershipPlan> {
    const record = this.#context.vaults.record(params.vault);
    const manifest = this.#context.vaults.manifest(params.vault);
    const config = await this.#context
      .wallet()
      .getVaultConfig(this.#context.vaults.address(params.vault));
    for (const index of params.remove)
      require(manifest.members.some((m) => m.index === index), `no member ${index} in this vault`);
    const joiners: Joiner[] = params.add.map((request) => decodeJoinRequest(request));
    for (const joiner of joiners)
      require(!manifest.members.some(
        (m) => m.identity === joiner.keeper.identity,
      ), `${joiner.name}'s keeper is already a member`);

    const staying = manifest.members.filter((m) => !params.remove.includes(m.index));
    const newMembers: NewMember[] = [
      ...staying.map((m) => ({
        identity: m.identity,
        transport: m.transport,
        addresses: m.addresses,
        signerKey: m.signerKey,
      })),
      ...joiners.map((j) => ({ ...j.keeper, signerKey: j.signerKey })),
    ];
    const n = newMembers.length;
    let failure: string | null = null;
    if (staying.length < manifest.viewingThreshold)
      failure = `at least ${manifest.viewingThreshold} members must stay to reshare the viewing key`;
    else if (params.threshold < 1 || params.threshold > n)
      failure = `the spending threshold must be 1 to ${n}`;
    else if (params.viewingThreshold < 2 || params.viewingThreshold > n)
      failure = `the viewing threshold must be 2 to ${n}`;
    const withinRemovalLimit = params.remove.length <= manifest.viewingThreshold - 1;
    if (!failure && !withinRemovalLimit)
      failure = `at most ${manifest.viewingThreshold - 1} members can leave in one rotation`;

    const names = {
      ...record.names,
      ...Object.fromEntries(joiners.map((j) => [j.keeper.identity, j.name])),
    };
    const accounts = {
      ...record.accounts,
      ...Object.fromEntries(joiners.map((j) => [j.keeper.identity, j.account])),
    };
    const plan: PlanFile = {
      v: 0,
      vault: record.address,
      name: record.name,
      session: `0x${randomBytes(32).toString("hex")}`,
      txNonce: Fr.random().toString(),
      planner: this.#context.vaults.myIndex(params.vault),
      remove: params.remove,
      // Exactly `t_v` dealers, all members who stay, so a leaving member never sees the new transcript.
      dealers: staying.slice(0, manifest.viewingThreshold).map((m) => m.index),
      threshold: params.threshold,
      viewingThreshold: params.viewingThreshold,
      newMembers,
      names,
      accounts,
      current: manifest,
      config,
      deployment: record.deployment,
    };
    const shape = (
      members: readonly { identity: string }[],
      threshold: number,
      viewingThreshold: number,
    ) => ({
      threshold,
      viewingThreshold,
      members: members.map((m) => names[m.identity] ?? "member"),
    });
    const membershipPlan: MembershipPlan = {
      id: plan.session,
      vault: record.address,
      current: shape(manifest.members, manifest.threshold, manifest.viewingThreshold),
      next: shape(newMembers, params.threshold, params.viewingThreshold),
      dealers: plan.dealers,
      joiners,
      withinRemovalLimit,
      failure,
      share: encodePlan(plan),
    };
    return membershipPlan;
  }

  /** Takes part in a rotation; the steps run in the background and report as `rotation` events. */
  async begin(text: string): Promise<RotationState> {
    const plan = decodePlan(text);
    const existing = this.#rotations.get(plan.session);
    if (existing && !existing.state.done) return existing.state;
    const me = this.#context.keeper.state.identity;
    const current = plan.current.members.find((m) => m.identity === me);
    const next = plan.newMembers.findIndex((m) => m.identity === me) + 1;
    if (!current && next === 0)
      throw new DaemonError("invalid", "this member takes no part in the rotation");
    if (current && next === 0)
      throw new DaemonError("invalid", "this member leaves in the rotation and has nothing to run");
    if (current) {
      const manifest = this.#context.vaults.manifest(plan.vault);
      require(manifest.epoch ===
        plan.current.epoch, "the plan starts from another committee epoch");
    }
    const role = !current
      ? "joins"
      : current.index === plan.planner
        ? "planner"
        : plan.dealers.includes(current.index)
          ? "dealer"
          : "stays";
    const state: RotationState = {
      plan: await this.#summary(plan),
      steps: STEPS.map((id) => ({ id, status: "pending", detail: null })),
      session: plan.session,
      proposal: null,
      error: null,
      done: false,
    };
    const rotating: Rotating = { state, plan, role, result: null };
    this.#rotations.set(plan.session, rotating);
    void this.#run(rotating);
    return state;
  }

  get(vault: Hex32, id: Hex32): RotationState | null {
    const rotating = this.#rotations.get(id.toLowerCase());
    return rotating && rotating.plan.vault === vault.toLowerCase() ? rotating.state : null;
  }

  list(vault: Hex32): RotationState[] {
    return [...this.#rotations.values()]
      .filter((r) => r.plan.vault === vault.toLowerCase())
      .map((r) => r.state);
  }

  /**
   * Approves a `rotate_membership` proposal of `vault` only for a rotation this daemon took part in, and only if the
   * proposal performs exactly the rotation this member's keeper reshared (`approveRotation` compares the signers and
   * the committee shares the reshare produced).
   */
  async approve(
    vault: Hex32,
    stored: Stored,
    tx: VaultTx,
    secret: Secret | undefined,
  ): Promise<string> {
    const rotating = [...this.#rotations.values()].find(
      (r) => r.plan.vault === vault && r.plan.txNonce === stored.info.txNonce && r.result,
    );
    if (!rotating)
      throw new DaemonError(
        "invalid",
        "this member took part in no reshare that this rotation matches",
      );
    const wallet = this.#context.wallet();
    const input = await wallet.vaultInput(tx.vault);
    const chain = await wallet.chain();
    const now = await wallet.chainTime();
    const feePayers = [this.#context.feePayer().toString()];
    const approval = this.#context.signer.with(secret, (signer) =>
      signer.approveRotation(
        input,
        chain,
        stored.body,
        stored.payload ?? "",
        this.#rotation(rotating),
        feePayers,
        now,
      ),
    );
    this.#mark(rotating, "approve", "done");
    return approval;
  }

  async #summary(plan: PlanFile): Promise<MembershipPlan> {
    const shape = (
      members: readonly { identity: string }[],
      threshold: number,
      viewingThreshold: number,
    ) => ({
      threshold,
      viewingThreshold,
      members: members.map((m) => plan.names[m.identity] ?? "member"),
    });
    const joiners: Joiner[] = plan.newMembers
      .filter((m) => !plan.current.members.some((c) => c.identity === m.identity))
      .map((m) => ({
        name: plan.names[m.identity] ?? "member",
        account: plan.accounts?.[m.identity] ?? `0x${"00".repeat(32)}`,
        signerKey: m.signerKey,
        keeper: { identity: m.identity, transport: m.transport, addresses: m.addresses },
      }));
    return {
      id: plan.session,
      vault: plan.vault,
      current: shape(plan.current.members, plan.current.threshold, plan.current.viewingThreshold),
      next: shape(plan.newMembers, plan.threshold, plan.viewingThreshold),
      dealers: plan.dealers,
      joiners,
      withinRemovalLimit: plan.remove.length <= plan.current.viewingThreshold - 1,
      failure: null,
      share: encodePlan(plan),
    };
  }

  /** The rotation the reshare produced, as `rotate_membership` performs it. */
  #rotation(rotating: Rotating): Rotation {
    const manifest = rotating.result?.manifest;
    if (!manifest) throw notReady("reshare", "the reshare has not finished");
    return {
      signers: rotating.plan.newMembers.map((m) => m.signerKey),
      threshold: rotating.plan.threshold,
      viewingThreshold: rotating.plan.viewingThreshold,
      verificationShares: manifest.members.map((m) => m.verificationShare),
    };
  }

  #mark(
    rotating: Rotating,
    id: RotationStepId,
    status: RotationStep["status"],
    detail: string | null = null,
  ): void {
    const step = rotating.state.steps.find((s) => s.id === id);
    if (step) {
      step.status = status;
      step.detail = detail ?? step.detail;
    }
    this.#context.emit({ type: "rotation", rotation: rotating.state });
    void writeVersioned(
      join(this.#context.paths.rotations, `${rotating.plan.session}.json`),
      ROTATION_SCHEMA,
      rotating,
    );
  }

  async #run(rotating: Rotating): Promise<void> {
    let step: RotationStepId = "reshare";
    try {
      await this.#reshare(rotating);
      step = "propose";
      await this.#propose(rotating);
      step = "approve";
      if (rotating.role !== "joins")
        this.#mark(rotating, "approve", "active", "approve the proposal to continue");
      step = "execute";
      await this.#activate(rotating);
      rotating.state.done = true;
      this.#mark(rotating, "activate", "done");
    } catch (error) {
      rotating.state.error = error instanceof Error ? error.message : String(error);
      rotating.state.done = true;
      this.#mark(rotating, step, "failed");
    }
  }

  async #reshare(rotating: Rotating): Promise<void> {
    const { plan, role } = rotating;
    const keeper = this.#context.keeper.client;
    this.#mark(rotating, "reshare", "active");
    const joining = plan.newMembers.some(
      (m) => !plan.current.members.some((c) => c.identity === m.identity),
    );
    const dealer =
      role !== "joins" && plan.dealers.includes(this.#context.vaults.myIndex(plan.vault));
    let secrets: string | undefined;
    if (dealer && joining) {
      const sealed = await this.#context.secrets().get(secretsName(plan.vault));
      if (!sealed) throw new DaemonError("failed", "this vault's protocol secrets are missing");
      const { transcripts } = this.#context.vaults.record(plan.vault);
      secrets = `0x${encodeHandoff(sealed, transcripts).toString("hex")}`;
    }
    await keeper.call("ceremony.reshare", {
      session: plan.session,
      vault: plan.vault,
      current: role === "joins" ? plan.current : null,
      dealers: plan.dealers,
      newThreshold: plan.viewingThreshold,
      newSpendingThreshold: plan.threshold,
      newMembers: plan.newMembers,
      ...(secrets ? { secrets } : {}),
    });
    const ceremony = await until(async () => {
      const c = await keeper.call("ceremony.status", { session: plan.session });
      if (c.status === "aborted")
        throw new DaemonError("failed", `the reshare aborted: ${c.failure?.reason}`);
      return c.status === "completed" || c.status === "activated" ? c : undefined;
    }, "the reshare");
    let result = ceremony.result as ReshareResult;
    if (role === "joins" && !result.secrets) {
      // The dealers' hand-off travels beside the sub-shares and may land just after the reshare completes.
      result = await until(
        async () => {
          const c = await keeper.call("ceremony.status", { session: plan.session });
          const r = c.result as ReshareResult | null;
          return r?.secrets ? r : undefined;
        },
        "the dealers' hand-off of the protocol secrets",
        60_000,
      );
    }
    rotating.result = result;
    const epoch = String(plan.config.version + 1);
    if (!result.transcript) throw new DaemonError("failed", "the reshare left this member out");
    if (role === "joins") {
      if (!result.secrets)
        throw new DaemonError(
          "failed",
          "no dealer handed this member the vault's protocol secrets",
        );
      const handoff = decodeHandoff(Buffer.from(result.secrets.slice(2), "hex"));
      const record: VaultRecord = {
        address: plan.vault,
        name: plan.name,
        foundedAt: Math.floor(Date.now() / 1000),
        founding: { lobby: null, session: plan.session, deployTx: null },
        deployment: plan.deployment,
        names: plan.names,
        accounts: plan.accounts ?? {},
        transcripts: { ...handoff.transcripts, [epoch]: result.transcript },
        config: null,
        status: "active",
      };
      await this.#context.secrets().put(secretsName(plan.vault), handoff.protocolSecrets);
      await this.#context.vaults.save({ ...record, status: "joining" });
    } else {
      const record = this.#context.vaults.record(plan.vault);
      record.transcripts[epoch] = result.transcript;
      record.names = { ...record.names, ...plan.names };
      record.accounts = { ...record.accounts, ...plan.accounts };
      await this.#context.vaults.save(record);
    }
    this.#mark(rotating, "reshare", "done", result.committeeCommitment);
  }

  /** The planner proposes `rotate_membership` under the plan's nonce; the others wait for that proposal. */
  async #propose(rotating: Rotating): Promise<void> {
    const { plan, role } = rotating;
    this.#mark(rotating, "propose", "active");
    if (role === "joins") {
      this.#mark(rotating, "propose", "done", "proposed by a member");
      return;
    }
    const keeper = this.#context.keeper.client;
    if (role === "planner") {
      rotating.state.proposal = await this.#proposeRotation(
        plan.vault,
        plan.current,
        this.#rotation(rotating),
        `rotation ${plan.session}`,
        plan.planner,
        Fr.fromString(plan.txNonce),
      );
    } else {
      rotating.state.proposal = await until(async () => {
        const bodies = await keeper.call("proposals.list", { vault: plan.vault });
        return bodies.map((b) => decodeProposal(b)).find((p) => p.txNonce === plan.txNonce)?.id;
      }, "the rotation proposal");
    }
    this.#mark(rotating, "propose", "done", rotating.state.proposal);
  }

  /**
   * Proposes replacing signer slot `slot`'s key with `signerKey`, keeping the committee and both thresholds: a signer
   * swap, which moves a member's signing key to another device. No reshare runs; after it executes, every member's
   * daemon has its keeper adopt the new key (`VaultService.observe`). Members approve it through `proposals.approve`.
   */
  async swapSigner(params: SwapSignerParams): Promise<Hex32> {
    const manifest = this.#context.vaults.manifest(params.vault);
    const slot = manifest.members[params.slot];
    require(slot !== undefined, `the vault has no signer slot ${params.slot}`);
    require(!manifest.members.some(
      (m) => m.signerKey.x === params.signerKey.x && m.signerKey.y === params.signerKey.y,
    ), "that key already signs for this vault");
    const rotation: Rotation = {
      signers: manifest.members.map((m, i) => (i === params.slot ? params.signerKey : m.signerKey)),
      threshold: manifest.threshold,
      viewingThreshold: manifest.viewingThreshold,
      verificationShares: manifest.members.map((m) => m.verificationShare),
    };
    const name = this.#context.vaults.record(params.vault).names[slot.identity];
    return this.#proposeRotation(
      params.vault,
      manifest,
      rotation,
      `move ${name ?? `member ${slot.index}`}'s signing key to a new device`,
      this.#context.vaults.myIndex(params.vault),
      Fr.random(),
    );
  }

  /** Builds `rotate_membership` from `current` to `rotation` with the fee call, stores it with the keeper, returns its id. */
  async #proposeRotation(
    vaultAddress: Hex32,
    current: Manifest,
    rotation: Rotation,
    memo: string,
    proposer: number,
    txNonce: Fr,
  ): Promise<Hex32> {
    await this.#context.vaults.ensureCurrent(vaultAddress);
    const wallet = this.#context.wallet();
    const vault = AztecAddress.fromStringUnsafe(vaultAddress);
    const method = Contract.at(vault, vaultArtifact(), wallet).methods.rotate_membership;
    if (!method) throw new DaemonError("failed", "the vault artifact has no rotate_membership");
    const rotate = await method(
      pad(current.members.map((m) => m.signerKey)),
      pad(rotation.signers),
      rotation.signers.length,
      rotation.threshold,
      rotation.viewingThreshold,
      pad(rotation.verificationShares),
      // The contract checks every share against this polynomial and its constant term against the vault's key.
      pad(committeePolynomial(rotation.viewingThreshold, rotation.verificationShares)),
    ).request();
    // The fee call first, as for every proposal (`payloadOf`).
    const payload = mergeExecutionPayloads([
      await this.#context.fee().paymentMethod.getExecutionPayload(),
      rotate,
    ]);
    const tx = await wallet.prepareVaultTx(vault, payload, {
      expiresAt: (await wallet.chainTime()) + 72 * 3600,
      summary: summaryOf({
        kind: "calls",
        calls: [{ to: vaultAddress, fn: "rotate_membership", args: [] }],
        memo,
      }),
      proposer,
      txNonce,
    });
    const keeper = this.#context.keeper.client;
    await keeper.call("payloads.put", { vault: vaultAddress, body: tx.payloadBody });
    await keeper.call("proposals.put", { vault: vaultAddress, body: tx.proposal });
    return tx.id as Hex32;
  }

  /**
   * The rotation this member's keeper reshared for the `rotate_membership` proposal with `txNonce`, if any: what a
   * paired device compares the proposal with.
   */
  reshared(vault: Hex32, txNonce: string): Rotation | null {
    const rotating = [...this.#rotations.values()].find(
      (r) => r.plan.vault === vault && r.plan.txNonce === txNonce && r.result,
    );
    return rotating ? this.#rotation(rotating) : null;
  }

  /**
   * Waits for the rotation to run, then has the keeper activate the new committee. Members read the new config note;
   * a joiner waits for the plan nonce's nullifier and hands its keeper the config the rotation sets.
   */
  async #activate(rotating: Rotating): Promise<void> {
    const { plan, role } = rotating;
    this.#mark(rotating, "activate", "active");
    const vault = AztecAddress.fromStringUnsafe(plan.vault);
    if (role !== "joins") {
      await until(
        async () => {
          const observation = await this.#context.vaults.observe(plan.vault).catch(() => undefined);
          return observation === "activated" ||
            this.#context.vaults.manifest(plan.vault).epoch > plan.current.epoch
            ? true
            : undefined;
        },
        "the rotation to run",
        3 * 24 * 3600 * 1000,
        2000,
      );
      this.#mark(rotating, "execute", "done");
      await this.#recordRemoval(plan);
      return;
    }
    const nullifier = await siloNullifier(
      vault,
      await poseidon2HashWithSeparator([Fr.fromString(plan.txNonce)], DOM_SEP_TX_NULLIFIER),
    );
    const node = this.#context.node.node;
    await until(
      async () => {
        const [found] = await node.findLeavesIndexes("latest", MerkleTreeId.NULLIFIER_TREE, [
          nullifier,
        ]);
        return found ? true : undefined;
      },
      "the rotation to run",
      3 * 24 * 3600 * 1000,
      2000,
    );
    this.#mark(rotating, "execute", "done");
    const config = checkRotation(
      { manifest: plan.current, config: plan.config },
      this.#rotation(rotating),
    );
    const { observation } = await this.#context.keeper.client.call("vault.observeConfig", {
      vault: plan.vault,
      config,
    });
    if (observation !== "activated")
      throw new DaemonError(
        "alarm",
        `the keeper did not activate the new committee (${observation})`,
      );
    const record = this.#context.vaults.record(plan.vault);
    record.status = "active";
    await this.#context.vaults.save(record);
    await this.#context.vaults.attach(plan.vault);
    await this.#recordRemoval(plan);
  }

  /**
   * Notes when a rotation that removed members took effect, and advises re-handshakes: counterparties who paid the vault
   * before it can still be noticed by the removed members until they handshake with the vault again.
   */
  async #recordRemoval(plan: PlanFile): Promise<void> {
    if (plan.remove.length === 0) return;
    const record = this.#context.vaults.record(plan.vault);
    // Chain time: received transfers are timed by their blocks.
    record.removals = [...(record.removals ?? []), await this.#context.wallet().chainTime()];
    await this.#context.vaults.save(record);
    const exposed = (await counterparties(this.#context, plan.vault).catch(() => [])).filter(
      (c) => c.exposed,
    );
    if (exposed.length > 0)
      this.#context.emit({
        type: "handshakeAdvised",
        vault: plan.vault,
        counterparties: exposed.length,
      });
  }
}
