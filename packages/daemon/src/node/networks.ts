import type { NodeStatus } from "@ome-labs/api";

/** A network Ome supports, recognised by its L1 chain and rollup, with the defaults a new profile gets there. */
export interface Network {
  readonly name: string;
  readonly l1ChainId: number;
  /** `null` matches any rollup: a local network's version changes with each start. */
  readonly rollupVersion: number | null;
  /** Relays a new profile uses, each `…/p2p/<peer id>`. */
  readonly relays: readonly string[];
}

/** Ome Labs' Testnet relay. Its identity lives with the relay operator; only the peer id is public. */
const OME_TESTNET_RELAY = "12D3KooWEKh8dk8ojB5SeCrWETZ3iYZxBK8hVVFE7ugALqsXXYWo";

export const NETWORKS: readonly Network[] = [
  { name: "local network", l1ChainId: 31_337, rollupVersion: null, relays: [] },
  {
    name: "Testnet",
    l1ChainId: 11_155_111,
    rollupVersion: 679_261_766,
    relays: [
      `/dns4/relay.ome.network/udp/4001/quic-v1/p2p/${OME_TESTNET_RELAY}`,
      `/dns4/relay.ome.network/tcp/4001/p2p/${OME_TESTNET_RELAY}`,
    ],
  },
];

/** Aztec's mainnet (Alpha) settles on Ethereum mainnet. */
const MAINNET_L1_CHAIN_ID = 1;

/** The supported network a node belongs to, if any. */
export function networkOf(node: NodeStatus): Network | undefined {
  return NETWORKS.find(
    (n) =>
      n.l1ChainId === node.l1ChainId &&
      (n.rollupVersion === null || n.rollupVersion === node.rollupVersion),
  );
}

/**
 * Why this release refuses a node, or `null` when it is supported. Ome is released for the local network and
 * Testnet; Alpha waits for the audit.
 */
export function unsupported(node: NodeStatus): string | null {
  if (node.l1ChainId === MAINNET_L1_CHAIN_ID)
    return "Ome is not released for Aztec's mainnet (Alpha) yet: use the local network or Testnet";
  if (!networkOf(node))
    return `this Ome release supports the local network and Testnet; the node at ${node.url} is on L1 chain ${node.l1ChainId ?? "?"}, rollup ${node.rollupVersion ?? "?"}`;
  return null;
}
