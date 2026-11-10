// The fee payer this member trusts: whose it is, its Fee Juice, and what a simulated transaction is expected to cost.
// The vault never pays its own fees; this payer does, so a payer that can't cover a transaction is refused before
// anyone spends a minute proving it.
import { getFeeJuiceBalance } from "@aztec-labs/aztec.js/utils";
import type { FeeEstimate, FeesStatus, Gas, Hex32 } from "@ome-labs/api";

import type { Context } from "./context.ts";
import { DaemonError } from "./errors.ts";

const ZERO_SALT = `0x${"00".repeat(32)}`;

/** Aztec's wallet pads the fee per gas by half against fee changes before inclusion (`minFeePadding`). */
const PADDING_PERCENT = 150n;

/** The fee payer, whose it is, and its Fee Juice; an unreadable balance is reported, not thrown. */
export async function feesStatus(context: Context): Promise<FeesStatus> {
  const payer = context.feePayer();
  const balance = await getFeeJuiceBalance(payer, context.node.node).catch(() => null);
  return {
    payer: payer.toString() as Hex32,
    kind: context.state.file.settings.feePayerSalt === ZERO_SALT ? "sponsored" : "own",
    feeJuice: balance === null ? null : balance.toString(),
    healthy: balance !== null && balance > 0n,
  };
}

/** What a transaction that used `gas` costs at the node's current minimum fees, and the most it may cost. */
export async function feeEstimate(context: Context, gas: Gas): Promise<FeeEstimate> {
  const fees = await context.node.node.getCurrentMinFees();
  const fee = BigInt(gas.da) * fees.feePerDaGas + BigInt(gas.l2) * fees.feePerL2Gas;
  return {
    gas,
    fee: fee.toString(),
    maxFee: ((fee * PADDING_PERCENT) / 100n).toString(),
    payer: context.feePayer().toString() as Hex32,
  };
}

/** Refuses with `insufficient-funds` when the fee payer's Fee Juice can't cover `estimate`'s expected cost. */
export async function requireFees(context: Context, estimate: FeeEstimate): Promise<void> {
  const status = await feesStatus(context);
  if (status.feeJuice === null || BigInt(status.feeJuice) >= BigInt(estimate.fee)) return;
  throw DaemonError.detailed(
    {
      kind: "insufficient-funds",
      funds: { type: "feeJuice", payer: status.payer },
      needed: estimate.fee,
      held: status.feeJuice,
    },
    `the fee payer ${status.payer} holds ${status.feeJuice} Fee Juice; this transaction is expected to cost ${estimate.fee}`,
  );
}
