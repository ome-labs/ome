// Typed arguments: each value is checked against its parameter's type, with the parameter's path in the refusal, and
// shaped as Aztec's encoder takes it; the encoder then gives the same fields as for hand-built values.
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { type AbiType as AztecType, encodeArguments } from "@aztec-labs/stdlib/abi";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { describe, expect, it } from "vitest";

import { apiType, apiValue, argument } from "./abi.ts";

const ADDRESS: AztecType = {
  kind: "struct",
  path: "aztec::protocol_types::address::aztec_address::AztecAddress",
  fields: [{ name: "inner", type: { kind: "field" } }],
};
const ORDER: AztecType = {
  kind: "struct",
  path: "amm::Order",
  fields: [
    { name: "amount", type: { kind: "integer", sign: "unsigned", width: 128 } },
    { name: "to", type: ADDRESS },
    { name: "path", type: { kind: "array", length: 2, type: { kind: "field" } } },
    { name: "exact", type: { kind: "boolean" } },
  ],
};
const TO = `0x${"0a".repeat(32)}`;

/** A check that `value` is refused for `type` at the path `f.x`. */
const refused = (type: AztecType, value: unknown) => () => argument(type, value as never, "f.x");

describe("argument", () => {
  it("shapes a struct of every kind as Aztec's encoder takes it", () => {
    const shaped = argument(
      ORDER,
      { amount: "1000", to: TO, path: ["1", "0x02"], exact: true },
      "swap.order",
    );
    const fields = encodeArguments(
      {
        name: "swap",
        functionType: "private",
        isOnlySelf: false,
        isStatic: false,
        isInitializer: false,
        parameters: [{ name: "order", type: ORDER, visibility: "private" }],
        returnTypes: [],
        errorTypes: {},
      } as never,
      [shaped],
    );
    expect(fields.map((f) => f.toBigInt())).toEqual([1000n, BigInt(TO), 1n, 2n, 1n]);
  });

  it("names the parameter it refuses", () => {
    expect(refused({ kind: "integer", sign: "unsigned", width: 8 }, "256")).toThrow(
      "f.x: u8 is from 0 to 255",
    );
    expect(refused({ kind: "integer", sign: "signed", width: 8 }, "-129")).toThrow(
      "i8 is from -128 to 127",
    );
    expect(refused({ kind: "field" }, "1.5")).toThrow("not a whole number");
    expect(refused({ kind: "boolean" }, "yes")).toThrow("true or false");
    expect(refused({ kind: "string", length: 3 }, "abcd")).toThrow("at most 3 characters");
    expect(refused({ kind: "array", length: 2, type: { kind: "field" } }, ["1"])).toThrow(
      "exactly 2",
    );
    expect(refused(ADDRESS, "0x12")).toThrow("64 hex characters");
    expect(refused(ORDER, { amount: "1", to: TO, path: ["1", "2"] })).toThrow("f.x.exact: missing");
    expect(
      refused(ORDER, { amount: "1", to: TO, path: ["1", "2"], exact: true, fee: "1" }),
    ).toThrow("has no field fee");
  });

  it("accepts signed integers and fields up to the modulus", () => {
    expect(argument({ kind: "integer", sign: "signed", width: 8 }, "-128", "x")).toBe(-128n);
    expect(() => argument({ kind: "field" }, Fr.MODULUS.toString(), "x")).toThrow("field modulus");
  });
});

describe("apiType and apiValue", () => {
  it("describe a known struct as one value and decoded values as text", () => {
    expect(apiType(ADDRESS)).toMatchObject({ kind: "struct", known: "aztecAddress" });
    expect(apiType(ORDER)).toMatchObject({
      kind: "struct",
      fields: [
        { name: "amount", type: { kind: "integer", signed: false, width: 128 } },
        { name: "to", type: { known: "aztecAddress" } },
        { name: "path", type: { kind: "array", length: 2 } },
        { name: "exact", type: { kind: "boolean" } },
      ],
    });
    expect(
      apiValue({
        amount: 5n,
        to: AztecAddress.fromStringUnsafe(TO),
        path: [new Fr(1n), new Fr(2n)],
        exact: false,
      }),
    ).toEqual({
      amount: "5",
      to: TO,
      path: [new Fr(1n).toString(), new Fr(2n).toString()],
      exact: false,
    });
  });
});
