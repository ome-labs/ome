import type {
  DeviceLabel,
  DeviceOffer,
  DeviceReviewSent,
  DeviceView,
  EnrollParams,
  Hex32,
  KeeperEvent,
  Point,
  ProposalRef,
} from "@ome-labs/api";
import { decodeProposal } from "@ome-labs/wasm";

import type { Context } from "../context.ts";
import { DaemonError, notReady, require } from "../errors.ts";
import type { MembershipService } from "../membership/service.ts";
import { peerId } from "../peer-id.ts";
import type { ProposalService } from "../proposals/service.ts";

/** How often the keeper's mailboxes are polled while a pairing or a device's vote is awaited. */
const FAST_POLL_MS = 3000;
/** Longest stretch of fast polling. */
const FAST_POLL_LIMIT_SECS = 600;

const samePoint = (a: Point, b: Point): boolean => a.x === b.x && a.y === b.y;

/**
 * The member's paired signing devices (`architecture/mobile.md`). The keeper pairs them and carries their reviews
 * and votes; the daemon decides what a device is asked: every proposal of a vault where the device holds this
 * member's key, sent with the config the daemon read, its labels and its simulation. While a pairing or a device's
 * vote is awaited, the daemon has the keeper poll its mailboxes every few seconds instead of every 30.
 */
export class DeviceService {
  readonly #context: Context;
  readonly #proposals: ProposalService;
  readonly #membership: MembershipService;
  /** Proposals already sent, by id. */
  readonly #forwarded = new Set<string>();
  #fastUntil = 0;
  #fastTimer: NodeJS.Timeout | undefined;

  constructor(context: Context, proposals: ProposalService, membership: MembershipService) {
    this.#context = context;
    this.#proposals = proposals;
    this.#membership = membership;
  }

  /** A one-time pairing offer; the daemon polls fast until it expires or a device pairs. */
  async pair(): Promise<DeviceOffer> {
    const profile = this.#context.state.file.profile;
    if (!profile) throw notReady("profile", "create a profile first");
    const offer = await this.#context.keeper.client.call("devices.offer", {
      member: profile.name,
      network: this.#context.node.network(),
      feePayers: [this.#context.feePayer().toString() as Hex32],
    });
    this.#pollFast(offer.expiresAt);
    return offer;
  }

  async list(): Promise<DeviceView[]> {
    const devices = await this.#context.keeper.client.call("devices.list", {});
    return devices.map((device) => ({
      identity: device.identity,
      name: device.name,
      signerKey: device.signerKey,
      pairedAt: device.pairedAt,
      peerId: peerId(device.identity),
      vaults: this.#context.vaults
        .list()
        .filter((r) => r.status === "active")
        .map((r) => r.address)
        .filter((vault) => {
          const key = this.#myKey(vault);
          return key !== undefined && samePoint(key, device.signerKey);
        }),
    }));
  }

  async revoke(identity: Hex32): Promise<void> {
    await this.#context.keeper.client.call("devices.revoke", { identity });
  }

  /** Proposes the signer swap that moves this member's key in `vault` to a paired device. Returns the proposal id. */
  async enroll(params: EnrollParams): Promise<Hex32> {
    const device = (await this.list()).find((d) => d.identity === params.identity.toLowerCase());
    if (!device) throw new DaemonError("not-found", `no paired device ${params.identity}`);
    require(!device.vaults.includes(
      params.vault.toLowerCase(),
    ), "this device already holds your key in this vault");
    return this.#membership.swapSigner({
      vault: params.vault,
      slot: this.#context.vaults.myIndex(params.vault) - 1,
      signerKey: device.signerKey,
    });
  }

  /** The paired device that holds this member's key in `vault`, if one does. */
  async holder(vault: Hex32): Promise<DeviceView | undefined> {
    return (await this.list()).find((d) => d.vaults.includes(vault.toLowerCase()));
  }

  /** Sends a proposal to the device holding this member's key in its vault, with what only the daemon knows. */
  async forward(ref: ProposalRef): Promise<DeviceReviewSent> {
    const vault = ref.vault.toLowerCase();
    const record = this.#context.vaults.record(vault);
    const wallet = this.#context.wallet();
    const review = await this.#proposals.review(ref);
    const proposal = review.proposal;
    const labels: DeviceLabel[] = [
      ...[...(await this.#context.names())].map(([address, label]) => ({ address, label })),
      ...(this.#context.state.file.tokens[vault] ?? []).map((t) => ({
        address: t.address,
        label: t.symbol,
      })),
    ].slice(0, 16);
    const rotation = this.#membership.reshared(vault, proposal.txNonce);
    const sent = await this.#context.keeper.client.call("devices.review", {
      vault,
      proposal: proposal.id,
      vaultName: record.name.slice(0, 64),
      config: await wallet.getVaultConfig(this.#context.vaults.address(vault)),
      chain: await wallet.chain(),
      labels,
      simulation: review.simulation.ok
        ? `ok, ${review.simulation.gas?.l2 ?? 0} L2 gas`
        : `failed: ${review.simulation.error ?? "unknown"}`.slice(0, 4096),
      ...(rotation ? { rotation } : {}),
    });
    this.#forwarded.add(proposal.id);
    this.#pollFast(
      Math.min(proposal.expiresAt, Math.floor(Date.now() / 1000) + FAST_POLL_LIMIT_SECS),
    );
    return sent;
  }

  /** Reacts to the keeper: a new proposal or payload is forwarded to the device holding this member's key. */
  onKeeperEvent(event: KeeperEvent): void {
    if (event.type === "proposal" || event.type === "payload")
      void this.#forwardPending(event.vault).catch(() => undefined);
  }

  /** Forwards every proposal of `vault` the device has not been sent yet, once its payload is here. */
  async #forwardPending(vault: Hex32): Promise<void> {
    if (!(await this.holder(vault))) return;
    const keeper = this.#context.keeper.client;
    const bodies = await keeper.call("proposals.list", { vault });
    const executed = new Set(
      (await keeper.call("receipts.list", { vault })).map((r) => r.proposal),
    );
    const now = Math.floor(Date.now() / 1000);
    for (const body of bodies) {
      const info = decodeProposal(body);
      if (this.#forwarded.has(info.id) || executed.has(info.id) || info.expiresAt < now) continue;
      // One proposal at a time: each forward reviews, simulates and deposits.
      // oxlint-disable-next-line no-await-in-loop
      const payload = await keeper.call("payloads.get", {
        vault,
        hash: info.payloadHash,
      });
      if (!payload) continue;
      // oxlint-disable-next-line no-await-in-loop
      await this.forward({ vault, id: info.id }).catch(() => undefined);
    }
  }

  /** This member's signer key in `vault`'s current committee. */
  #myKey(vault: Hex32): Point | undefined {
    try {
      const index = this.#context.vaults.myIndex(vault);
      return this.#context.vaults.manifest(vault).members[index - 1]?.signerKey;
    } catch {
      return undefined;
    }
  }

  /** Polls the keeper's mailboxes every few seconds until `until` (unix seconds), at most ten minutes ahead. */
  #pollFast(until: number): void {
    this.#fastUntil = Math.max(
      this.#fastUntil,
      Math.min(until, Math.floor(Date.now() / 1000) + FAST_POLL_LIMIT_SECS),
    );
    if (this.#fastTimer) return;
    const tick = (): void => {
      if (Date.now() / 1000 >= this.#fastUntil) {
        this.#fastTimer = undefined;
        return;
      }
      void this.#context.keeper.client
        .call("mailbox.poll", {})
        .catch(() => undefined)
        .finally(() => {
          this.#fastTimer = setTimeout(tick, FAST_POLL_MS);
          this.#fastTimer.unref();
        });
    };
    this.#fastTimer = setTimeout(tick, FAST_POLL_MS);
    this.#fastTimer.unref();
  }

  stop(): void {
    clearTimeout(this.#fastTimer);
    this.#fastTimer = undefined;
    this.#fastUntil = 0;
  }
}
