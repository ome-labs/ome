import { Contract } from "@aztec-labs/aztec.js/contracts";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import { FunctionType } from "@aztec-labs/stdlib/abi";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { type ExecutionPayload, mergeExecutionPayloads } from "@aztec-labs/stdlib/tx";
import type { Hex32, Intent, IntentAuthwit, IntentCall } from "@ome-labs/api";
import { type VaultAuthwit, vaultArtifact } from "@ome-labs/wallet";

import type { Context } from "../context.ts";
import { argument } from "../contracts/abi.ts";
import { DaemonError, require } from "../errors.ts";
import { LIMITS } from "../limits.ts";

const FUNCTION = /^[A-Za-z_][A-Za-z0-9_]{0,63}$/u;

/** The summary a proposal carries: its intent as JSON, which every member's daemon reads back. */
export function summaryOf(intent: Intent): string {
  const summary = JSON.stringify(intent);
  require(Buffer.byteLength(summary) <=
    LIMITS.maxSummaryBytes, `the proposal's description is ${Buffer.byteLength(summary)} bytes; a proposal carries at most ${LIMITS.maxSummaryBytes}`);
  return summary;
}

export function intentOf(summary: string): Intent {
  try {
    const intent = JSON.parse(summary) as Intent;
    if (intent && typeof intent === "object" && "kind" in intent) return intent;
  } catch {
    // Not an Ome intent: show the summary as a memo.
  }
  return { kind: "calls", calls: [], memo: summary };
}

/**
 * The execution payload an intent describes, with the trusted fee payer's call merged in (the vault never pays its own
 * fees). A cancellation is the fee call alone: run under the cancelled proposal's nonce, it burns that nonce.
 *
 * The fee call comes first: the fee payer ends Aztec's setup phase, and a public call enqueued before that runs as
 * setup, which only allow-listed functions may (`Setup function not on allow list`).
 */
export async function payloadOf(
  context: Context,
  intent: Intent,
  vault: AztecAddress,
): Promise<ExecutionPayload> {
  const wallet = context.wallet();
  const fee = await context.fee().paymentMethod.getExecutionPayload();
  switch (intent.kind) {
    case "transfer": {
      require(BigInt(intent.amount) > 0n, "a transfer moves a positive amount");
      const token = TokenContract.at(AztecAddress.fromStringUnsafe(intent.token), wallet);
      const transfer = await token.methods
        .transfer(AztecAddress.fromStringUnsafe(intent.to), BigInt(intent.amount))
        .request();
      return mergeExecutionPayloads([fee, transfer]);
    }
    case "calls": {
      require(intent.calls.length >= 1 &&
        intent.calls.length <=
          LIMITS.maxCalls, `a proposal makes 1 to ${LIMITS.maxCalls} calls besides its fee`);
      const payloads = await Promise.all(intent.calls.map((call) => callPayload(context, call)));
      return mergeExecutionPayloads([fee, ...payloads]);
    }
    case "cancel":
      return mergeExecutionPayloads([fee]);
    case "setThreshold": {
      const config = await wallet.getVaultConfig(vault);
      require(Number.isInteger(intent.threshold) &&
        intent.threshold >= 1 &&
        intent.threshold <=
          config.signerCount, `the approvals to spend are 1 to ${config.signerCount}, the vault's members`);
      require(intent.threshold !==
        config.threshold, `the vault already needs ${config.threshold} approvals to spend`);
      const call = await vaultCall(context, vault, "set_threshold", [intent.threshold]);
      return mergeExecutionPayloads([fee, call]);
    }
    case "invalidatePending": {
      const call = await vaultCall(context, vault, "invalidate_pending", []);
      return mergeExecutionPayloads([fee, call]);
    }
    default:
      throw new DaemonError("invalid", "unknown intent");
  }
}

/**
 * The private authwits an intent grants, each call built from its contract's artifact and private. Whether the
 * transaction uses each one is read from its simulation (`ProposalService`), so a caller may be any contract the
 * calls reach: an AMM behind a router.
 */
export async function authwitsOf(context: Context, intent: Intent): Promise<VaultAuthwit[]> {
  if (intent.kind !== "calls") return [];
  const authwits: readonly IntentAuthwit[] = intent.authwits ?? [];
  require(authwits.length <=
    LIMITS.maxAuthwits, `a proposal grants at most ${LIMITS.maxAuthwits} authwits`);
  return Promise.all(
    authwits.map(async ({ caller, call }) => {
      const [fn] = (await callPayload(context, call)).calls;
      if (!fn) throw new DaemonError("invalid", `${call.fn} built no call`);
      require(fn.type ===
        FunctionType.PRIVATE, `${call.fn} is not a private function; a private authwit authorizes a private call`);
      return { caller: AztecAddress.fromStringUnsafe(caller), call: fn };
    }),
  );
}

/** One call, its arguments checked against the function's parameters before Aztec encodes them. */
async function callPayload(context: Context, call: IntentCall): Promise<ExecutionPayload> {
  require(FUNCTION.test(call.fn), `${call.fn} is not a function name`);
  const { artifact } = await context.contracts.artifact(call.to.toLowerCase() as Hex32);
  const fn = artifact.functions.find((f) => f.name === call.fn);
  if (!fn) throw new DaemonError("invalid", `${artifact.name} has no function ${call.fn}`);
  require(call.args.length ===
    fn.parameters
      .length, `${call.fn} takes ${fn.parameters.length} arguments (${fn.parameters.map((p) => p.name).join(", ")}); ${call.args.length} given`);
  const args = fn.parameters.map((param, i) =>
    argument(param.type, call.args[i] as IntentCall["args"][number], `${call.fn}.${param.name}`),
  );
  const contract = Contract.at(AztecAddress.fromStringUnsafe(call.to), artifact, context.wallet());
  const method = contract.methods[call.fn];
  if (!method) throw new DaemonError("invalid", `${artifact.name} has no function ${call.fn}`);
  return method(...args).request();
}

/** A call to one of the vault's own functions, which run only through its approved transactions. */
async function vaultCall(
  context: Context,
  vault: AztecAddress,
  fn: string,
  args: unknown[],
): Promise<ExecutionPayload> {
  const method = Contract.at(vault, vaultArtifact(), context.wallet()).methods[fn];
  if (!method) throw new DaemonError("failed", `the vault's artifact has no ${fn}`);
  return method(...args).request();
}
