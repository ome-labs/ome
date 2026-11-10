import { Contract } from "@aztec-labs/aztec.js/contracts";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { type ExecutionPayload, mergeExecutionPayloads } from "@aztec-labs/stdlib/tx";
import type { Hex32, Intent, IntentCall } from "@ome-labs/api";

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
 */
export async function payloadOf(context: Context, intent: Intent): Promise<ExecutionPayload> {
  const wallet = context.wallet();
  const fee = await context.fee().paymentMethod.getExecutionPayload();
  switch (intent.kind) {
    case "transfer": {
      require(BigInt(intent.amount) > 0n, "a transfer moves a positive amount");
      const token = TokenContract.at(AztecAddress.fromStringUnsafe(intent.token), wallet);
      const transfer = await token.methods
        .transfer(AztecAddress.fromStringUnsafe(intent.to), BigInt(intent.amount))
        .request();
      return mergeExecutionPayloads([transfer, fee]);
    }
    case "calls": {
      require(intent.calls.length >= 1 &&
        intent.calls.length <=
          LIMITS.maxCalls, `a proposal makes 1 to ${LIMITS.maxCalls} calls besides its fee`);
      const payloads = await Promise.all(intent.calls.map((call) => callPayload(context, call)));
      return mergeExecutionPayloads([...payloads, fee]);
    }
    case "cancel":
      return mergeExecutionPayloads([fee]);
    default:
      throw new DaemonError("invalid", "unknown intent");
  }
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
