// What a vault member signs for a transaction: function selectors, argument and calldata hashes, the account
// entrypoint's app payload hash (calls padded to Aztec's maximum) and the authwit outer hash that binds it to the vault,
// chain and rollup version, and the message hashes of private authwits granted beside a transaction. Computed with
// Aztec's own entrypoint encoder and aztec.js's authwit helpers; ome-protocol must reproduce every value.

import {
  computeAuthWitMessageHash,
  computeInnerAuthWitHashFromAction,
} from "@aztec-labs/aztec.js/authorization";
import { EncodedAppEntrypointCalls } from "@aztec-labs/entrypoints/encoding";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { FunctionCall, FunctionSelector, FunctionType } from "@aztec-labs/stdlib/abi";
import { computeOuterAuthWitHash } from "@aztec-labs/stdlib/auth-witness";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { computeCalldataHash, computeVarArgsHash } from "@aztec-labs/stdlib/hash";

import { hex } from "../encoding.ts";

const SIGNATURES = {
  privateTransfer: "transfer_private_to_private((Field),(Field),u128,Field)",
  publicTransfer: "transfer_public_to_public((Field),(Field),u128,Field)",
  setThreshold: "set_threshold(u32)",
  constructor: "constructor()",
};

interface CallSpec {
  signature: string;
  to: bigint;
  type: "private" | "public";
  isStatic: boolean;
  hideMsgSender: boolean;
  args: bigint[];
}

const CALLS = {
  privateTransfer: {
    signature: SIGNATURES.privateTransfer,
    to: 0x1001n,
    type: "private",
    isStatic: false,
    hideMsgSender: false,
    args: [0x2001n, 0x2002n, 10n, 7n],
  },
  publicTransfer: {
    signature: SIGNATURES.publicTransfer,
    to: 0x1002n,
    type: "public",
    isStatic: false,
    hideMsgSender: true,
    args: [0x2001n, 0x2003n, 5n, 0n],
  },
  staticRead: {
    signature: SIGNATURES.constructor,
    to: 0x1003n,
    type: "public",
    isStatic: true,
    hideMsgSender: false,
    args: [],
  },
  selfCall: {
    signature: SIGNATURES.setThreshold,
    to: 0x1234n,
    type: "private",
    isStatic: false,
    hideMsgSender: false,
    args: [3n],
  },
  privateNoArgs: {
    signature: SIGNATURES.constructor,
    to: 0x1004n,
    type: "private",
    isStatic: false,
    hideMsgSender: false,
    args: [],
  },
} satisfies Record<string, CallSpec>;

const PAYLOADS: Record<string, { calls: (keyof typeof CALLS)[]; txNonce: bigint }> = {
  empty: { calls: [], txNonce: 1n },
  single: { calls: ["privateTransfer"], txNonce: 0xabcdn },
  mixed: { calls: ["privateTransfer", "publicTransfer", "staticRead"], txNonce: 42n },
  full: {
    calls: ["selfCall", "privateTransfer", "publicTransfer", "staticRead", "selfCall"],
    txNonce: 0xffffn,
  },
};

/** Private authwits: an AMM pulling the vault's tokens, and a call without arguments. */
const AUTHWITS: Record<string, { caller: bigint; call: keyof typeof CALLS }> = {
  transfer: { caller: 0x3001n, call: "privateTransfer" },
  noArgs: { caller: 0x3002n, call: "privateNoArgs" },
};

const CONTEXT = { vault: 0x1234n, chainId: 31337n, version: 1n };

async function functionCall(spec: CallSpec): Promise<FunctionCall> {
  return FunctionCall.from({
    name: spec.signature.slice(0, spec.signature.indexOf("(")),
    to: new AztecAddress(new Fr(spec.to)),
    selector: await FunctionSelector.fromSignature(spec.signature),
    type: spec.type === "public" ? FunctionType.PUBLIC : FunctionType.PRIVATE,
    isStatic: spec.isStatic,
    hideMsgSender: spec.hideMsgSender,
    args: spec.args.map((a) => new Fr(a)),
    returnType: undefined,
  });
}

const encodeCall = (spec: CallSpec) => ({
  ...spec,
  to: hex(new Fr(spec.to)),
  args: spec.args.map((a) => hex(new Fr(a))),
});

async function payload(calls: CallSpec[], txNonce: bigint) {
  const encoded = await EncodedAppEntrypointCalls.create(
    await Promise.all(calls.map(functionCall)),
    new Fr(txNonce),
  );
  const payloadHash = await encoded.hash();
  const outerHash = await computeOuterAuthWitHash(
    new AztecAddress(new Fr(CONTEXT.vault)),
    new Fr(CONTEXT.chainId),
    new Fr(CONTEXT.version),
    payloadHash,
  );
  return {
    calls: calls.map(encodeCall),
    txNonce: hex(new Fr(txNonce)),
    fields: encoded.toFields().map(hex),
    payloadHash: hex(payloadHash),
    outerHash: hex(outerHash),
  };
}

async function authwit(caller: bigint, spec: CallSpec) {
  const call = await functionCall(spec);
  const intent = { caller: new AztecAddress(new Fr(caller)), call };
  return {
    caller: hex(new Fr(caller)),
    call: encodeCall(spec),
    innerHash: hex(await computeInnerAuthWitHashFromAction(intent.caller, call)),
    messageHash: hex(
      await computeAuthWitMessageHash(intent, {
        chainId: new Fr(CONTEXT.chainId),
        version: new Fr(CONTEXT.version),
      }),
    ),
  };
}

export async function payloadVectors() {
  const selectors = await Promise.all(
    Object.values(SIGNATURES).map(async (signature) => ({
      signature,
      selector: (await FunctionSelector.fromSignature(signature)).toString(),
    })),
  );
  const args = [[], [1n], [1n, 2n, 3n, 4n, 5n]];
  return {
    context: {
      vault: hex(new Fr(CONTEXT.vault)),
      chainId: hex(new Fr(CONTEXT.chainId)),
      version: hex(new Fr(CONTEXT.version)),
    },
    selectors,
    argsHashes: await Promise.all(
      args.map(async (values) => ({
        args: values.map((v) => hex(new Fr(v))),
        argsHash: hex(await computeVarArgsHash(values.map((v) => new Fr(v)))),
        calldataHash: hex(await computeCalldataHash(values.map((v) => new Fr(v)))),
      })),
    ),
    authwits: Object.fromEntries(
      await Promise.all(
        Object.entries(AUTHWITS).map(async ([name, { caller, call }]) => [
          name,
          await authwit(caller, CALLS[call]),
        ]),
      ),
    ),
    payloads: Object.fromEntries(
      await Promise.all(
        Object.entries(PAYLOADS).map(async ([name, { calls, txNonce }]) => [
          name,
          await payload(
            calls.map((c): CallSpec => CALLS[c]),
            txNonce,
          ),
        ]),
      ),
    ),
  };
}
