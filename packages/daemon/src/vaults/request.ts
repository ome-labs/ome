// Payment requests: an `ome-pay:` link a member hands a payer (architecture/wire.md). It names the vault, the network,
// and optionally the token, amount and memo asked for. A payer's wallet pays the vault with its default strategy, a
// non-interactive handshake, which every member's daemon finds; the link needs no sender registration.
import type { Hex32, PaymentRequest, PaymentRequestParams } from "@ome-labs/api";

import type { Context } from "../context.ts";
import { DaemonError, require } from "../errors.ts";

const SCHEME = "ome-pay:";
/** The link format this release writes; a link without `v` is format 0. */
const FORMAT = "0";
const HEX32 = /^0x[0-9a-f]{64}$/u;
const AMOUNT = /^[1-9][0-9]*$/u;

const notALink = () => new DaemonError("invalid", "not an ome-pay: link");

/** Builds the request for `params`; the token, when given, must be one the vault tracks. */
export function paymentRequest(context: Context, params: PaymentRequestParams): PaymentRequest {
  const vault = context.vaults.record(params.vault).address;
  const tokens = context.state.file.tokens[vault] ?? [];
  const token = params.token === undefined ? null : params.token.toLowerCase();
  const info = token === null ? null : (tokens.find((t) => t.address === token) ?? null);
  if (token !== null && !info)
    throw new DaemonError("not-found", `the vault doesn't track ${token}: add it first`);
  if (params.amount !== undefined)
    require(AMOUNT.test(params.amount), "an amount is a positive whole number of base units");
  if (params.memo !== undefined)
    require(params.memo.length <= 280, "a memo is at most 280 characters");
  const request = {
    vault,
    network: context.node.network(),
    token: info,
    amount: params.amount ?? null,
    memo: params.memo ?? null,
  };
  return { uri: formatRequest(request), ...request };
}

/** The `ome-pay:` link of a request. */
export function formatRequest(request: Omit<PaymentRequest, "uri">): string {
  const query = new URLSearchParams({ v: FORMAT, network: request.network });
  if (request.token) query.set("token", request.token.address);
  if (request.amount) query.set("amount", request.amount);
  if (request.memo) query.set("memo", request.memo);
  return `${SCHEME}${request.vault}?${query.toString()}`;
}

/** The fields of an `ome-pay:` link; refuses one that isn't well formed. */
export function parseRequest(uri: string): {
  vault: Hex32;
  network: string;
  token: Hex32 | null;
  amount: string | null;
  memo: string | null;
} {
  if (!uri.startsWith(SCHEME)) throw notALink();
  const [vault, query = ""] = uri.slice(SCHEME.length).split("?", 2);
  const params = new URLSearchParams(query);
  const format = params.get("v") ?? FORMAT;
  if (format !== FORMAT)
    throw new DaemonError(
      "invalid",
      `this payment link needs a newer release of Ome (format v${format})`,
    );
  const network = params.get("network");
  const token = params.get("token");
  const amount = params.get("amount");
  if (!vault || !HEX32.test(vault) || !network) throw notALink();
  if (token !== null && !HEX32.test(token)) throw notALink();
  if (amount !== null && !AMOUNT.test(amount)) throw notALink();
  return {
    vault: vault as Hex32,
    network,
    token: token as Hex32 | null,
    amount,
    memo: params.get("memo"),
  };
}
