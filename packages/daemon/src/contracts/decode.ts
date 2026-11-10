// A payload's calls as reviews show them: named, typed arguments decoded with the contract's artifact, and labels for
// what the member knows (the vault, its tokens, aztec-wallet aliases). Without the artifact the arguments stay raw
// fields, marked as not decoded.
import {
  type ContractArtifact,
  FunctionSelector,
  type FunctionCall,
  FunctionType,
  decodeEachFromAbi,
} from "@aztec-labs/stdlib/abi";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { AbiType, DecodedArg, DecodedCall, Hex32, TokenInfo } from "@ome-labs/api";

import type { Context } from "../context.ts";
import { tokenUnits } from "../proposals/summary.ts";
import { apiType, apiValue } from "./abi.ts";

/** What the member knows names for. */
export interface Labels {
  readonly vault: Hex32;
  readonly vaultName: string;
  readonly tokens: readonly TokenInfo[];
  /** aztec-wallet aliases, by address. */
  readonly names: ReadonlyMap<Hex32, string>;
}

const lower = (value: { toString(): string }) => value.toString().toLowerCase() as Hex32;
const AMOUNT = /amount|value/iu;

/** A name for `address`: the vault, a token, or an alias. */
export function labelOf(address: Hex32, labels: Labels): string | null {
  if (address === labels.vault) return `${labels.vaultName} (this vault)`;
  const token = labels.tokens.find((t) => t.address === address);
  if (token) return token.symbol;
  const alias = labels.names.get(address);
  return alias ? `${alias} (alias)` : null;
}

async function artifactOf(context: Context, to: Hex32): Promise<ContractArtifact | undefined> {
  const pxe = context.wallet().pxeService;
  const instance = await pxe.getContractInstance(AztecAddress.fromStringUnsafe(to));
  return instance ? pxe.getContractArtifact(instance.originalContractClassId) : undefined;
}

export async function decodeCalls(
  context: Context,
  calls: readonly FunctionCall[],
  labels: Labels,
): Promise<DecodedCall[]> {
  return Promise.all(
    calls.map(async (call) => {
      const to = lower(call.to);
      const base = {
        to,
        toLabel: to === labels.vault ? labels.vaultName : labelOf(to, labels),
        fn: call.name,
        selector: call.selector.toString(),
        kind: call.type === FunctionType.PRIVATE ? ("private" as const) : ("public" as const),
        isStatic: call.isStatic,
      };
      const artifact = await artifactOf(context, to).catch(() => undefined);
      const fn = artifact ? await functionOf(artifact, call) : undefined;
      if (!fn)
        return {
          ...base,
          decoded: false,
          args: call.args.map((arg, i) => ({
            name: `arg${i}`,
            type: { kind: "field" as const },
            value: arg.toString(),
            display: null,
          })),
        };
      const token = labels.tokens.find((t) => t.address === to);
      const values = decodeEachFromAbi(
        fn.parameters.map((p) => p.type),
        call.args,
      );
      const args: DecodedArg[] = fn.parameters.map((param, i) => {
        const type = apiType(param.type);
        const value = apiValue(values[i]);
        return {
          name: param.name,
          type,
          value,
          display: displayOf(param.name, type, value, token, labels),
        };
      });
      return { ...base, decoded: true, args };
    }),
  );
}

/** The function a call runs: by name, confirmed by its selector. */
async function functionOf(artifact: ContractArtifact, call: FunctionCall) {
  for (const fn of artifact.functions.filter((f) => f.name === call.name)) {
    // oxlint-disable-next-line no-await-in-loop
    const selector = await FunctionSelector.fromNameAndParameters(fn.name, fn.parameters);
    if (selector.equals(call.selector)) return fn;
  }
  return undefined;
}

/** How a person reads an argument, when the member knows better than the raw value. */
function displayOf(
  name: string,
  type: AbiType,
  value: DecodedArg["value"],
  token: TokenInfo | undefined,
  labels: Labels,
): string | null {
  if (typeof value !== "string") return null;
  if (type.kind === "struct" && type.known === "aztecAddress") return labelOf(lower(value), labels);
  if (type.kind === "integer" && token && AMOUNT.test(name) && !value.startsWith("-"))
    return `${tokenUnits(value, token.decimals)} ${token.symbol}`;
  return null;
}
