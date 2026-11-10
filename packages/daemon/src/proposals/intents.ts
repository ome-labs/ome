import { Contract } from "@aztec-labs/aztec.js/contracts";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import type { ContractArtifact } from "@aztec-labs/stdlib/abi";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { type ExecutionPayload, mergeExecutionPayloads } from "@aztec-labs/stdlib/tx";
import type { Hex32, Intent } from "@ome-labs/api";

import type { Context } from "../context.ts";
import { DaemonError, require } from "../errors.ts";

/** Largest summary a proposal carries (the wire limit is 4 KiB). */
const MAX_SUMMARY = 4096;

const FUNCTION = /^[A-Za-z_][A-Za-z0-9_]{0,63}$/u;

/** A call argument as the CLI types it: `0x` hex or a decimal integer as a field, anything else as a string. */
function argument(value: string): Fr | bigint | string {
  if (/^0x[0-9a-fA-F]{1,64}$/u.test(value)) return Fr.fromString(value);
  if (/^-?\d+$/u.test(value)) return BigInt(value);
  return value;
}

/** The artifact of a contract registered in this member's PXE. */
export async function artifactOf(
  context: Context,
  address: AztecAddress,
): Promise<ContractArtifact> {
  const pxe = context.wallet().pxeService;
  const instance = await pxe.getContractInstance(address);
  const artifact = instance
    ? await pxe.getContractArtifact(instance.originalContractClassId)
    : undefined;
  if (!artifact)
    throw DaemonError.detailed(
      {
        kind: "unknown-contract",
        address: address.toString() as Hex32,
        ...(instance ? { classId: instance.originalContractClassId.toString() as Hex32 } : {}),
      },
      `contract ${address.toString()} is not registered with this member`,
    );
  return artifact;
}

/** The summary a proposal carries: its intent as JSON, which every member's daemon reads back. */
export function summaryOf(intent: Intent): string {
  const summary = JSON.stringify(intent);
  require(summary.length <= MAX_SUMMARY, "the proposal's description is too long");
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
        intent.calls.length <= 4, "a proposal makes 1 to 4 calls besides its fee");
      const payloads = await Promise.all(
        intent.calls.map(async (call) => {
          require(FUNCTION.test(call.fn), `${call.fn} is not a function name`);
          const to = AztecAddress.fromStringUnsafe(call.to);
          const method = Contract.at(to, await artifactOf(context, to), wallet).methods[call.fn];
          if (!method) throw new DaemonError("invalid", `the contract has no function ${call.fn}`);
          return method(...call.args.map(argument)).request();
        }),
      );
      return mergeExecutionPayloads([...payloads, fee]);
    }
    case "cancel":
      return mergeExecutionPayloads([fee]);
    default:
      throw new DaemonError("invalid", "unknown intent");
  }
}
