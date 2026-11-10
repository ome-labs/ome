// An ome-pay: link carries the vault, the network and what is asked for, and parses back to the same fields.
import type { Hex32 } from "@ome-labs/api";
import { describe, expect, it } from "vitest";

import { formatRequest, parseRequest } from "./request.ts";

const VAULT = `0x${"0a".repeat(32)}` as Hex32;
const TOKEN = `0x${"0c".repeat(32)}` as Hex32;

describe("ome-pay: links", () => {
  it("round-trip the vault, network, token, amount and memo", () => {
    const uri = formatRequest({
      vault: VAULT,
      network: "aztec:31337:1",
      token: { address: TOKEN, name: "Ome Test", symbol: "OMET", decimals: 18, standard: true },
      amount: "2500000000000000000",
      memo: "March rent & fees",
    });
    expect(uri).toBe(
      `ome-pay:${VAULT}?v=0&network=aztec%3A31337%3A1&token=${TOKEN}&amount=2500000000000000000&memo=March+rent+%26+fees`,
    );
    expect(parseRequest(uri)).toEqual({
      vault: VAULT,
      network: "aztec:31337:1",
      token: TOKEN,
      amount: "2500000000000000000",
      memo: "March rent & fees",
    });
  });

  it("asks for nothing beyond the vault and network when nothing is given", () => {
    const uri = formatRequest({
      vault: VAULT,
      network: "aztec:31337:1",
      token: null,
      amount: null,
      memo: null,
    });
    expect(parseRequest(uri)).toMatchObject({ token: null, amount: null, memo: null });
  });

  it("refuses links that aren't well formed", () => {
    expect(() => parseRequest(`ome-pay:${VAULT}`)).toThrow(/ome-pay/u);
    expect(() => parseRequest(`ome-pay:0x12?network=aztec:1:1`)).toThrow(/ome-pay/u);
    expect(() => parseRequest(`ome-pay:${VAULT}?network=aztec:1:1&amount=-5`)).toThrow(/ome-pay/u);
    expect(() => parseRequest(`pay:${VAULT}?network=aztec:1:1`)).toThrow(/ome-pay/u);
  });

  it("reads a link without a format as format 0, and asks for an update for a newer one", () => {
    expect(parseRequest(`ome-pay:${VAULT}?network=aztec:1:1`)).toMatchObject({
      network: "aztec:1:1",
    });
    expect(() => parseRequest(`ome-pay:${VAULT}?v=2&network=aztec:1:1`)).toThrow(
      /needs a newer release of Ome \(format v2\)/u,
    );
  });
});
