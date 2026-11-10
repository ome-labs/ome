// Typed call arguments: Aztec's ABI types as the API describes them, values checked against a type before Aztec's
// encoder sees them (so a mistake names its parameter), and decoded values back as the API's values.
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { EthAddress } from "@aztec-labs/foundation/eth-address";
import {
  type AbiDecoded,
  type AbiType as AztecType,
  FunctionSelector,
  isAztecAddressStruct,
  isEthAddressStruct,
  isFunctionSelectorStruct,
} from "@aztec-labs/stdlib/abi";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { AbiType, AbiValue, KnownType } from "@ome-labs/api";

import { DaemonError } from "../errors.ts";

const HEX = /^0x[0-9a-fA-F]+$/u;
const DECIMAL = /^-?\d+$/u;
const FIELD_MODULUS = Fr.MODULUS;

function known(type: AztecType): KnownType | undefined {
  if (isAztecAddressStruct(type)) return "aztecAddress";
  if (isEthAddressStruct(type)) return "ethAddress";
  if (isFunctionSelectorStruct(type)) return "functionSelector";
  return undefined;
}

/** An Aztec ABI type as the API describes it. */
export function apiType(type: AztecType): AbiType {
  switch (type.kind) {
    case "field":
      return { kind: "field" };
    case "boolean":
      return { kind: "boolean" };
    case "integer":
      return { kind: "integer", signed: type.sign === "signed", width: type.width };
    case "array":
      return { kind: "array", length: type.length, item: apiType(type.type) };
    case "string":
      return { kind: "string", length: type.length };
    case "struct": {
      const tag = known(type);
      return {
        kind: "struct",
        path: type.path,
        fields: type.fields.map((field) => ({ name: field.name, type: apiType(field.type) })),
        ...(tag ? { known: tag } : {}),
      };
    }
    case "tuple":
      return { kind: "tuple", items: type.fields.map(apiType) };
  }
}

function refuse(path: string, message: string): never {
  throw new DaemonError("invalid", `${path}: ${message}`);
}

function text(value: AbiValue, path: string, what: string): string {
  if (typeof value !== "string") refuse(path, `${what} is text`);
  return value.trim();
}

/** A whole number from decimal or `0x` hex text. */
function integer(value: AbiValue, path: string): bigint {
  const raw = text(value, path, "a number");
  if (!HEX.test(raw) && !DECIMAL.test(raw)) refuse(path, `"${raw}" is not a whole number`);
  return BigInt(raw);
}

/**
 * `value` checked against `type` and shaped as Aztec's encoder takes it: fields and integers as bigints, booleans,
 * strings, addresses and selectors as their classes, arrays and tuples as lists, structs as objects by field name.
 */
export function argument(type: AztecType, value: AbiValue, path: string): unknown {
  switch (type.kind) {
    case "field": {
      const number = integer(value, path);
      if (number < 0n || number >= FIELD_MODULUS)
        refuse(path, "a field is from 0 to the field modulus");
      return new Fr(number);
    }
    case "boolean":
      if (typeof value === "boolean") return value;
      if (value === "true" || value === "false") return value === "true";
      return refuse(path, "a bool is true or false");
    case "integer": {
      const number = integer(value, path);
      const signed = type.sign === "signed";
      const min = signed ? -(1n << BigInt(type.width - 1)) : 0n;
      const max = signed ? (1n << BigInt(type.width - 1)) - 1n : (1n << BigInt(type.width)) - 1n;
      if (number < min || number > max)
        refuse(path, `${signed ? "i" : "u"}${type.width} is from ${min} to ${max}`);
      return number;
    }
    case "string": {
      const raw = text(value, path, "a string");
      if ([...raw].length > type.length) refuse(path, `at most ${type.length} characters`);
      return raw;
    }
    case "array":
      if (!Array.isArray(value) || value.length !== type.length)
        refuse(path, `a list of exactly ${type.length}`);
      return value.map((item, i) => argument(type.type, item, `${path}[${i}]`));
    case "tuple":
      if (!Array.isArray(value) || value.length !== type.fields.length)
        refuse(path, `a list of exactly ${type.fields.length}`);
      return type.fields.map((item, i) => argument(item, value[i] as AbiValue, `${path}.${i}`));
    case "struct":
      return struct(type, value, path);
  }
}

function struct(
  type: Extract<AztecType, { kind: "struct" }>,
  value: AbiValue,
  path: string,
): unknown {
  if (isAztecAddressStruct(type)) {
    const raw = text(value, path, "an address");
    if (!/^0x[0-9a-fA-F]{64}$/u.test(raw))
      refuse(path, "an Aztec address is 0x and 64 hex characters");
    return AztecAddress.fromStringUnsafe(raw);
  }
  if (isEthAddressStruct(type)) {
    const raw = text(value, path, "an address");
    if (!/^0x[0-9a-fA-F]{40}$/u.test(raw))
      refuse(path, "an Ethereum address is 0x and 40 hex characters");
    return EthAddress.fromString(raw);
  }
  if (isFunctionSelectorStruct(type)) {
    const raw = text(value, path, "a selector");
    if (!/^0x[0-9a-fA-F]{8}$/u.test(raw)) refuse(path, "a selector is 0x and 8 hex characters");
    return FunctionSelector.fromString(raw);
  }
  if (typeof value !== "object" || value === null || Array.isArray(value))
    refuse(path, "a struct is an object of its fields");
  const fields = value as Record<string, AbiValue>;
  const names = new Set(type.fields.map((field) => field.name));
  const extra = Object.keys(fields).find((name) => !names.has(name));
  if (extra) refuse(path, `has no field ${extra}`);
  return Object.fromEntries(
    type.fields.map((field) => {
      const given = fields[field.name];
      if (given === undefined) refuse(`${path}.${field.name}`, "missing");
      return [field.name, argument(field.type, given, `${path}.${field.name}`)];
    }),
  );
}

/** A value Aztec's decoder returned, as the API's value: numbers and addresses as text. */
export function apiValue(decoded: AbiDecoded): AbiValue {
  if (decoded === undefined) return "";
  if (typeof decoded === "boolean") return decoded;
  if (typeof decoded === "bigint") return decoded.toString();
  if (typeof decoded === "string") return decoded;
  if (Array.isArray(decoded)) return decoded.map(apiValue);
  if (decoded instanceof Fr) return decoded.toString();
  if (
    decoded instanceof AztecAddress ||
    decoded instanceof EthAddress ||
    decoded instanceof FunctionSelector
  )
    return decoded.toString();
  return Object.fromEntries(Object.entries(decoded).map(([name, item]) => [name, apiValue(item)]));
}
