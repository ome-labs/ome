// Generated from crates/wasm by `pnpm --filter @ome-labs/wasm generate`. Do not edit.

import type {
  ErrorKind,
  Hex32,
  Hex64,
  HexBytes,
  Manifest,
  MemberPartials,
  Point,
  Secret,
  VaultConfig,
} from "@ome-labs/api";

/**
 * A vault as the bindings check proposals against it: its committee manifest and the config note read from the
 * chain.
 */
export type VaultInput = {
  /**
   * The committee manifest the keeper serves.
   */
  manifest: Manifest;
  /**
   * The vault's config note.
   */
  config: VaultConfig;
};

/**
 * The chain a transaction is bound to, from the node.
 */
export type Chain = {
  /**
   * L1 chain id, as a field.
   */
  chainId: Hex32;
  /**
   * Rollup version, as a field.
   */
  version: Hex32;
};

/**
 * How a viewing share maps to a share of the address secret.
 */
export type AddressContext = {
  /**
   * The vault address.
   */
  address: Hex32;
  /**
   * `1` or `-1`: the sign Aztec applies to the address secret.
   */
  sign: number;
  /**
   * `preaddress · G + Ivpk`, normalised to positive `y`.
   */
  addressPoint: Point;
};

/**
 * A threshold ECDH batch to verify and combine: the keeper's `ecdh.batch` responses with what they answer.
 */
export type CombineRequest = {
  /**
   * Committee epoch the partials are bound to.
   */
  epoch: number;
  /**
   * Vault preaddress.
   */
  preaddress: Hex32;
  /**
   * Group incoming viewing key `Ivpk`.
   */
  ivpk: Point;
  /**
   * Verification shares `Y_i` in member order.
   */
  verificationShares: Array<Point>;
  /**
   * Viewing threshold `t_v`.
   */
  threshold: number;
  /**
   * Input points, in the order the partials answer them.
   */
  inputs: Array<Point>;
  /**
   * Members' partials.
   */
  responses: Array<MemberPartials>;
};

/**
 * Combined points.
 */
export type CombineResult = {
  /**
   * `address_secret · input` for each input, in order.
   */
  points: Array<Point>;
  /**
   * Members whose response was malformed or carried an invalid proof.
   */
  invalidMembers: Array<number>;
};

/**
 * Where a call runs.
 */
export type CallKind = "private" | "public";

/**
 * One call of a vault transaction.
 */
export type Call = {
  /**
   * The function signature, when known; it must hash to `selector`.
   */
  signature: string | null;
  /**
   * Target contract.
   */
  to: Hex32;
  /**
   * Function selector.
   */
  selector: number;
  /**
   * Private or public.
   */
  kind: CallKind;
  /**
   * The call may not modify state.
   */
  isStatic: boolean;
  /**
   * Hide the vault as `msg_sender` (public calls only).
   */
  hideMsgSender: boolean;
  /**
   * Encoded arguments.
   */
  args: Array<Hex32>;
};

/**
 * The calls a payload carries, and the PXE's execution data.
 */
export type ExecutionRequest = {
  /**
   * At most five calls; none is a cancellation.
   */
  calls: Array<Call>;
  /**
   * Execution data the signatures do not cover.
   */
  extras: HexBytes;
};

/**
 * What a member proposes.
 */
export type Draft = {
  /**
   * The calls.
   */
  calls: Array<Call>;
  /**
   * Execution data the signatures do not cover.
   */
  extras: HexBytes;
  /**
   * Entrypoint nonce; cancelling burns it.
   */
  txNonce: Hex32;
  /**
   * Approval deadline, Unix seconds.
   */
  expiresAt: number;
  /**
   * Summary shown to the members.
   */
  summary: string;
  /**
   * Proposing member (1-based).
   */
  proposer: number;
};

/**
 * A proposal ready to store with the keeper (`proposals.put`, `payloads.put`).
 */
export type BuiltProposal = {
  /**
   * Proposal id.
   */
  id: Hex32;
  /**
   * The proposal's canonical encoding.
   */
  proposal: HexBytes;
  /**
   * The payload's canonical encoding.
   */
  payload: HexBytes;
  /**
   * Outer hash the approvals sign over.
   */
  outerHash: Hex32;
  /**
   * The message an approval signs.
   */
  approvalMessage: Hex32;
};

/**
 * A decoded proposal.
 */
export type ProposalInfo = {
  /**
   * Proposal id.
   */
  id: Hex32;
  /**
   * Config version it was made under.
   */
  configVersion: number;
  /**
   * Outer hash.
   */
  outerHash: Hex32;
  /**
   * Deadline, Unix seconds.
   */
  expiresAt: number;
  /**
   * Entrypoint nonce.
   */
  txNonce: Hex32;
  /**
   * Fee method; Ome proposals use only `0` (external).
   */
  feeMethod: number;
  /**
   * Hash of the referenced payload.
   */
  payloadHash: Hex32;
  /**
   * Length of the referenced payload.
   */
  payloadLen: number;
  /**
   * Summary.
   */
  summary: string;
  /**
   * Proposing member.
   */
  proposer: number;
};

/**
 * A decoded approval.
 */
export type ApprovalInfo = {
  /**
   * Proposal id.
   */
  proposal: Hex32;
  /**
   * Signer slot, 0-based.
   */
  slot: number;
  /**
   * Schnorr signature.
   */
  signature: Hex64;
};

/**
 * A decoded rejection.
 */
export type RejectionInfo = {
  /**
   * Proposal id.
   */
  proposal: Hex32;
  /**
   * Signer slot, 0-based.
   */
  slot: number;
  /**
   * Reason shown to the members.
   */
  reason: string;
  /**
   * Schnorr signature.
   */
  signature: Hex64;
};

/**
 * Where a proposal stands.
 */
export type TallyStatus = "pending" | "approved" | "blocked" | "expired";

/**
 * A vote the tally refused.
 */
export type RefusedVote = {
  /**
   * Position in the approvals or rejections given.
   */
  index: number;
  /**
   * Whether it was an approval.
   */
  approval: boolean;
  /**
   * Why.
   */
  reason: string;
};

/**
 * The votes on a proposal.
 */
export type TallyResult = {
  /**
   * Where it stands.
   */
  status: TallyStatus;
  /**
   * Counted approvals.
   */
  approvals: number;
  /**
   * Counted rejections.
   */
  rejections: number;
  /**
   * The spending threshold `k`.
   */
  needed: number;
  /**
   * Votes that did not verify.
   */
  refused: Array<RefusedVote>;
  /**
   * The 49-field auth witness, once approved.
   */
  witness: Array<Hex32> | null;
};

/**
 * A proposed membership rotation: the new signers, spending threshold and committee from the reshare.
 */
export type Rotation = {
  /**
   * New signer keys in slot order.
   */
  signers: Array<Point>;
  /**
   * New spending threshold `k`.
   */
  threshold: number;
  /**
   * New viewing threshold `t_v`.
   */
  viewingThreshold: number;
  /**
   * The verification shares the reshare produced.
   */
  verificationShares: Array<Point>;
};

/**
 * A threshold choice members should make knowingly.
 */
export type Warning = "single-signer" | "no-spare-signer" | "two-can-read" | "no-spare-viewer";

/**
 * A committee dealt from one secret, for tests (`@ome-labs/wasm/testing`).
 */
export type DealtCommittee = {
  /**
   * Group key: the secret times the generator.
   */
  groupKey: Point;
  /**
   * Members' viewing shares, in member order.
   */
  shares: Array<Hex32>;
  /**
   * Members' verification shares, in member order.
   */
  verificationShares: Array<Point>;
};

/** What every function throws: an `Error` named `OmeWasmError` with the API error kind. */
export interface OmeWasmError extends Error {
  name: "OmeWasmError";
  kind: ErrorKind;
}
