// Which nodes this release accepts, and the relays a new profile gets on each.
import type { NodeStatus } from "@ome-labs/api";
import { describe, expect, it } from "vitest";

import { networkOf, unsupported } from "./networks.ts";

const node = (l1ChainId: number | null, rollupVersion: number | null): NodeStatus => ({
  url: "http://node",
  reachable: true,
  nodeVersion: "6.0.0",
  l1ChainId,
  rollupVersion,
  blockNumber: 1,
  syncedBlock: 1,
  checkedAt: 0,
  error: null,
});

describe("networks", () => {
  it("accepts a local network of any rollup version, with no relays", () => {
    expect(unsupported(node(31_337, 42))).toBeNull();
    expect(networkOf(node(31_337, 7))?.relays).toEqual([]);
  });

  it("accepts Testnet and gives a new profile Ome Labs' relay", () => {
    const testnet = node(11_155_111, 679_261_766);
    expect(unsupported(testnet)).toBeNull();
    expect(networkOf(testnet)?.relays).toHaveLength(2);
    expect(networkOf(testnet)?.relays.every((r) => r.startsWith("/dns4/relay.ome.network/"))).toBe(
      true,
    );
  });

  it("refuses Aztec's mainnet and networks it does not know", () => {
    expect(unsupported(node(1, 1))).toMatch(/not released for Aztec's mainnet/u);
    expect(unsupported(node(11_155_111, 1))).toMatch(/supports the local network and Testnet/u);
  });
});
