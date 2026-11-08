import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { type AbiType, FunctionCall, FunctionSelector, FunctionType } from "@aztec-labs/stdlib/abi";
import { AuthWitness } from "@aztec-labs/stdlib/auth-witness";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { Capsule, ExecutionPayload, HashedValues } from "@aztec-labs/stdlib/tx";
import type { HexBytes } from "@ome-labs/api";
import type { Call, ExecutionRequest } from "@ome-labs/wasm";

/**
 * What a payload carries besides the calls: data the PXE needs to execute them but the signatures do not cover. It
 * cannot change which calls run or with which arguments; contracts must constrain any hint they read from it.
 */
interface Extras {
  readonly v: 0;
  /** Per call: the function name and return type, for building and decoding the call. */
  readonly names: string[];
  readonly returnTypes: (AbiType | null)[];
  readonly authWitnesses: string[];
  readonly capsules: string[];
  readonly extraHashedArgs: string[];
  readonly feePayer: string | null;
}

const hex = (bytes: Uint8Array): HexBytes => `0x${Buffer.from(bytes).toString("hex")}`;
const bytes = (value: HexBytes): Buffer => Buffer.from(value.slice(2), "hex");

/**
 * The calls and extras of an Aztec execution payload, as an Ome proposal carries them (`buildProposal`). Each call's
 * target, selector, kind, arguments and flags are signed; utility calls cannot run through the entrypoint.
 */
export function toVaultRequest(payload: ExecutionPayload): ExecutionRequest {
  const calls: Call[] = payload.calls.map((call) => {
    if (call.type === FunctionType.UTILITY)
      throw new Error(`${call.name} is a utility function; it cannot run through the vault`);
    return {
      signature: null,
      to: call.to.toString(),
      selector: call.selector.value,
      kind: call.type === FunctionType.PRIVATE ? "private" : "public",
      isStatic: call.isStatic,
      hideMsgSender: call.hideMsgSender,
      args: call.args.map((arg) => arg.toString()),
    };
  });
  const extras: Extras = {
    v: 0,
    names: payload.calls.map((call) => call.name),
    returnTypes: payload.calls.map((call) => call.returnType ?? null),
    authWitnesses: payload.authWitnesses.map((w) => w.toString()),
    capsules: payload.capsules.map((c) => c.toString()),
    extraHashedArgs: payload.extraHashedArgs.map((h) => hex(h.toBuffer())),
    feePayer: payload.feePayer?.toString() ?? null,
  };
  return { calls, extras: hex(Buffer.from(JSON.stringify(extras), "utf8")) };
}

/** The Aztec execution payload a checked proposal describes (the inverse of {@link toVaultRequest}). */
export function fromVaultRequest(request: ExecutionRequest): ExecutionPayload {
  const extras = JSON.parse(bytes(request.extras).toString("utf8")) as Extras;
  if (extras.v !== 0 || extras.names.length !== request.calls.length)
    throw new Error("Unknown or inconsistent payload extras");
  const calls = request.calls.map(
    (call, i) =>
      new FunctionCall(
        extras.names[i] ?? "",
        AztecAddress.fromStringUnsafe(call.to),
        new FunctionSelector(call.selector),
        call.kind === "private" ? FunctionType.PRIVATE : FunctionType.PUBLIC,
        call.hideMsgSender,
        call.isStatic,
        call.args.map((arg) => Fr.fromString(arg)),
        extras.returnTypes[i] ?? undefined,
      ),
  );
  return new ExecutionPayload(
    calls,
    extras.authWitnesses.map((w) => AuthWitness.fromString(w)),
    extras.capsules.map((c) => Capsule.fromString(c)),
    extras.extraHashedArgs.map((h) => HashedValues.fromBuffer(bytes(h))),
    extras.feePayer ? AztecAddress.fromStringUnsafe(extras.feePayer) : undefined,
  );
}
