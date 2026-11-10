import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { type AbiType, FunctionCall, FunctionSelector, FunctionType } from "@aztec-labs/stdlib/abi";
import { AuthWitness } from "@aztec-labs/stdlib/auth-witness";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { Capsule, ExecutionPayload, HashedValues } from "@aztec-labs/stdlib/tx";
import type { HexBytes } from "@ome-labs/api";
import type { Authwit, Call, ExecutionRequest } from "@ome-labs/wasm";

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
  /** What members need to decode, simulate and prove the calls; absent from payloads that name none. */
  readonly contracts?: PayloadContracts;
}

/**
 * The contract classes a payload's calls run, and the instances of its targets. A member's daemon fetches the classes'
 * artifacts from the proposer's keeper and checks each against its class id; an instance is checked against its
 * address. Both are hints: neither can change which calls run.
 */
export interface PayloadContracts {
  readonly classIds: string[];
  /**
   * Every contract the proposer's simulation reached, nested calls included, so members register their instances
   * before simulating. Absent when only the calls' own targets are needed.
   */
  readonly addresses?: string[];
  /** `SerializableContractInstance` bytes with the address, as hex, for targets whose instance isn't published. */
  readonly instances: { readonly address: string; readonly instance: HexBytes }[];
}

const NO_CONTRACTS: PayloadContracts = { classIds: [], instances: [] };

/**
 * A private authwit the vault grants with a transaction: `caller`, a contract the transaction reaches, may make `call`
 * on the vault's behalf once. Members sign its message hash beside the transaction's.
 */
export interface VaultAuthwit {
  readonly caller: AztecAddress;
  readonly call: FunctionCall;
}

const hex = (bytes: Uint8Array): HexBytes => `0x${Buffer.from(bytes).toString("hex")}`;
const bytes = (value: HexBytes): Buffer => Buffer.from(value.slice(2), "hex");

/**
 * The calls and extras of an Aztec execution payload, as an Ome proposal carries them (`buildProposal`). Each call's
 * target, selector, kind, arguments and flags are signed; utility calls cannot run through the entrypoint.
 */
export function toVaultRequest(
  payload: ExecutionPayload,
  contracts: PayloadContracts = NO_CONTRACTS,
  authwits: readonly VaultAuthwit[] = [],
): ExecutionRequest {
  const calls = payload.calls.map(toCall);
  const extras: Extras = {
    v: 0,
    names: payload.calls.map((call) => call.name),
    returnTypes: payload.calls.map((call) => call.returnType ?? null),
    authWitnesses: payload.authWitnesses.map((w) => w.toString()),
    capsules: payload.capsules.map((c) => c.toString()),
    extraHashedArgs: payload.extraHashedArgs.map((h) => hex(h.toBuffer())),
    feePayer: payload.feePayer?.toString() ?? null,
    ...(contracts.classIds.length > 0 ||
    contracts.instances.length > 0 ||
    (contracts.addresses?.length ?? 0) > 0
      ? { contracts }
      : {}),
  };
  return {
    calls,
    authwits: authwits.map((a): Authwit => ({ caller: a.caller.toString(), call: toCall(a.call) })),
    extras: hex(Buffer.from(JSON.stringify(extras), "utf8")),
  };
}

function toCall(call: FunctionCall): Call {
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
}

const fromCall = (call: Call, name = "", returnType?: AbiType) =>
  new FunctionCall(
    name,
    AztecAddress.fromStringUnsafe(call.to),
    new FunctionSelector(call.selector),
    call.kind === "private" ? FunctionType.PRIVATE : FunctionType.PUBLIC,
    call.hideMsgSender,
    call.isStatic,
    call.args.map((arg) => Fr.fromString(arg)),
    returnType,
  );

/** The authwits a checked request grants (the inverse of {@link toVaultRequest}'s `authwits`). */
export function vaultAuthwits(request: ExecutionRequest): VaultAuthwit[] {
  return request.authwits.map((a) => ({
    caller: AztecAddress.fromStringUnsafe(a.caller),
    call: fromCall(a.call),
  }));
}

/** The contracts a checked request's calls need (see {@link PayloadContracts}). */
export function payloadContracts(request: ExecutionRequest): PayloadContracts {
  const extras = JSON.parse(bytes(request.extras).toString("utf8")) as Extras;
  return extras.contracts ?? NO_CONTRACTS;
}

/** The Aztec execution payload a checked proposal describes (the inverse of {@link toVaultRequest}). */
export function fromVaultRequest(request: ExecutionRequest): ExecutionPayload {
  const extras = JSON.parse(bytes(request.extras).toString("utf8")) as Extras;
  if (extras.v !== 0 || extras.names.length !== request.calls.length)
    throw new Error("Unknown or inconsistent payload extras");
  const calls = request.calls.map((call, i) =>
    fromCall(call, extras.names[i] ?? "", extras.returnTypes[i] ?? undefined),
  );
  return new ExecutionPayload(
    calls,
    extras.authWitnesses.map((w) => AuthWitness.fromString(w)),
    extras.capsules.map((c) => Capsule.fromString(c)),
    extras.extraHashedArgs.map((h) => HashedValues.fromBuffer(bytes(h))),
    extras.feePayer ? AztecAddress.fromStringUnsafe(extras.feePayer) : undefined,
  );
}
