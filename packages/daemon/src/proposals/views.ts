import type {
  DecodedAuthwit,
  DecodedCall,
  Hex32,
  HexBytes,
  ProposalStatus,
  ProposalView,
  TokenInfo,
  Vote,
} from "@ome-labs/api";
import { type VaultAuthwit, fromVaultRequest, vaultAuthwits } from "@ome-labs/wallet";
import {
  type ProposalInfo,
  approvalMessage,
  checkProposal,
  decodeApproval,
  decodeProposal,
  decodeRejection,
  tally,
  verifyApproval,
  verifyRejection,
} from "@ome-labs/wasm";

import type { Context } from "../context.ts";
import { type Labels, decodeCalls, labelOf } from "../contracts/decode.ts";
import { intentOf } from "./intents.ts";

/** Everything stored about one proposal, gathered from the member's keeper. */
export interface Stored {
  readonly vault: Hex32;
  readonly body: HexBytes;
  readonly info: ProposalInfo;
  readonly payload: HexBytes | null;
  readonly approvals: HexBytes[];
  readonly rejections: HexBytes[];
  /** When this member's keeper stored each vote, by body. */
  readonly voteTimes: ReadonlyMap<HexBytes, number>;
}

/** The proposals of `vault`, with their payloads and votes, from the member's keeper. */
export async function storedProposals(context: Context, vault: Hex32): Promise<Stored[]> {
  const keeper = context.keeper.client;
  const bodies = await keeper.call("proposals.list", { vault });
  return Promise.all(
    bodies.map(async (body) => {
      const info = decodeProposal(body);
      const [payload, approvals, rejections] = await Promise.all([
        keeper.call("payloads.get", { vault, hash: info.payloadHash }),
        keeper.call("approvals.list", { proposal: info.id }),
        keeper.call("rejections.list", { proposal: info.id }),
      ]);
      return {
        vault,
        body,
        info,
        payload,
        approvals: approvals.map((vote) => vote.body),
        rejections: rejections.map((vote) => vote.body),
        voteTimes: new Map([...approvals, ...rejections].map((vote) => [vote.body, vote.at])),
      };
    }),
  );
}

const TALLY: Record<string, ProposalStatus> = {
  pending: "collecting",
  approved: "ready",
  blocked: "blocked",
  expired: "expired",
};

/**
 * A proposal as this member sees it: votes verified against the vault's signers, status from the tally at chain
 * time, executions from the keeper's receipts, and cancellations from executed proposals that spent the same nonce.
 */
export async function proposalView(
  context: Context,
  stored: Stored,
  all: readonly Stored[],
  tokens: readonly TokenInfo[],
): Promise<ProposalView> {
  const { vault, body, info } = stored;
  const record = context.vaults.record(vault);
  const manifest = context.vaults.manifest(vault);
  const wallet = context.wallet();
  const config =
    record.config?.config ?? (await wallet.getVaultConfig(context.vaults.address(vault)));
  const input = { manifest, config };
  const now = await wallet.chainTime();
  const [receipts, claims] = await Promise.all([
    context.keeper.client.call("receipts.list", { vault }),
    context.keeper.client.call("claims.list", { vault }),
  ]);
  const executed = new Map(receipts.map((r) => [r.proposal, r]));
  const nameOf = (index: number) => {
    const member = manifest.members.find((m) => m.index === index);
    return member ? (record.names[member.identity] ?? `member ${index}`) : `member ${index}`;
  };
  const memberRef = (index: number) => ({ index, name: nameOf(index) });
  const at = (vote: HexBytes) => stored.voteTimes.get(vote) ?? 0;

  const approvals: Vote[] = stored.approvals.map((a) => {
    const { slot } = decodeApproval(a);
    let valid = true;
    try {
      verifyApproval(input, body, a);
    } catch {
      valid = false;
    }
    return {
      slot,
      index: slot + 1,
      name: nameOf(slot + 1),
      approve: true,
      valid,
      reason: null,
      at: at(a),
    };
  });
  const approvedSlots = new Set(approvals.filter((v) => v.valid).map((v) => v.slot));
  const rejections: Vote[] = stored.rejections
    .map((r) => {
      const { slot, reason } = decodeRejection(r);
      let valid = true;
      try {
        verifyRejection(input, body, r);
      } catch {
        valid = false;
      }
      return {
        slot,
        index: slot + 1,
        name: nameOf(slot + 1),
        approve: false,
        valid,
        reason,
        at: at(r),
      };
    })
    .filter((v) => v.valid && !approvedSlots.has(v.slot));

  let calls: ProposalView["calls"] = [];
  let authwits: ProposalView["authwits"] = [];
  const messages = [info.outerHash, ...info.authwits].map((hash) =>
    approvalMessage(vault, hash, info.configVersion, info.expiresAt),
  );
  if (stored.payload) {
    try {
      const request = checkProposal(
        input,
        await wallet.chain(),
        body,
        stored.payload,
        Math.min(now, info.expiresAt),
      );
      const labels = {
        vault,
        vaultName: record.name,
        tokens,
        names: await context.names(),
        members: context.vaults.memberAccounts(vault),
      };
      calls = await decodeCalls(context, fromVaultRequest(request).calls, labels);
      authwits = await decodeAuthwits(
        context,
        {
          authwits: vaultAuthwits(request),
          authwitHashes: info.authwits,
          approvalMessages: messages,
        },
        calls,
        labels,
      );
    } catch {
      // A stale proposal no longer checks against the vault; its status says why.
    }
  }

  const intent = intentOf(info.summary);
  const spent = all.find(
    (other) =>
      other.info.id !== info.id &&
      other.info.txNonce === info.txNonce &&
      executed.has(other.info.id),
  );
  let status: ProposalStatus;
  if (executed.has(info.id)) status = "executed";
  else if (spent) status = "cancelled";
  else if (info.configVersion < config.version) status = "stale";
  else {
    const result = tally(input, body, stored.approvals, stored.rejections, now);
    status = TALLY[result.status] ?? "collecting";
  }
  const mySlot = context.vaults.myIndex(vault) - 1;
  const receipt = executed.get(info.id);
  // The newest live claim: who is executing it right now, if anyone.
  const claim = receipt
    ? undefined
    : claims.filter((c) => c.proposal === info.id).toSorted((a, b) => b.updatedAt - a.updatedAt)[0];
  const cancelledBy =
    intent.kind === "cancel"
      ? intent.cancels
      : (all.find((o) => {
          const other = intentOf(o.info.summary);
          return other.kind === "cancel" && other.cancels === info.id;
        })?.info.id ?? null);
  return {
    id: info.id,
    vault,
    intent,
    proposer: { index: info.proposer, name: nameOf(info.proposer) },
    status,
    threshold: config.threshold,
    members: manifest.members.length,
    votes: [...approvals, ...rejections],
    mySlot,
    approvedByMe: approvals.some((v) => v.valid && v.slot === mySlot),
    rejectedByMe: rejections.some((v) => v.slot === mySlot),
    configVersion: info.configVersion,
    currentConfigVersion: config.version,
    createdAt: info.createdAt,
    expiresAt: info.expiresAt,
    txNonce: info.txNonce,
    outerHash: info.outerHash,
    approvalMessage: messages[0] ?? info.outerHash,
    feeMethod: info.feeMethod === 0 ? "external" : `method ${info.feeMethod}`,
    calls,
    authwits,
    token:
      intent.kind === "transfer" ? (tokens.find((t) => t.address === intent.token) ?? null) : null,
    execution: receipt
      ? {
          txHash: receipt.txHash,
          at: receipt.at,
          executor: receipt.executor === null ? null : memberRef(receipt.executor),
          block: receipt.block || null,
        }
      : null,
    executing: claim
      ? {
          member: memberRef(claim.member),
          phase: claim.phase,
          since: claim.startedAt,
          mine: claim.mine,
        }
      : null,
    cancelledBy,
  };
}

/**
 * A proposal's authwits as reviews show them: each call decoded like the transaction's, the caller named as the call
 * it is (the caller is always one of them), and the hash and message an approval signs for it.
 */
export async function decodeAuthwits(
  context: Context,
  tx: {
    readonly authwits: readonly VaultAuthwit[];
    readonly authwitHashes: readonly { toString(): string }[];
    readonly approvalMessages: readonly string[];
  },
  calls: readonly DecodedCall[],
  labels: Labels,
): Promise<DecodedAuthwit[]> {
  const decoded = await decodeCalls(
    context,
    tx.authwits.map((a) => a.call),
    labels,
  );
  return tx.authwits.map((a, i) => {
    const caller = a.caller.toString().toLowerCase() as Hex32;
    const call = decoded[i];
    if (!call) throw new Error("an authwit did not decode");
    return {
      caller,
      callerLabel: calls.find((c) => c.to === caller)?.toLabel ?? labelOf(caller, labels),
      call,
      messageHash: (tx.authwitHashes[i]?.toString() ?? "") as Hex32,
      approvalMessage: (tx.approvalMessages[i + 1] ?? "") as Hex32,
    };
  });
}
