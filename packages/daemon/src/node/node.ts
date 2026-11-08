import { createAztecNodeClient } from "@aztec-labs/aztec.js/node";
import type { AztecNode } from "@aztec-labs/stdlib/interfaces/client";
import type { NodeStatus } from "@ome-labs/api";

/** The Aztec node a profile talks to, and what the last check found. */
export class NodeService {
  readonly node: AztecNode;
  readonly url: string;
  #status: NodeStatus;

  constructor(url: string) {
    this.url = url;
    this.node = createAztecNodeClient(url);
    this.#status = blank(url);
  }

  get status(): NodeStatus {
    return this.#status;
  }

  /** Checks the node now. */
  async check(): Promise<NodeStatus> {
    try {
      const [info, block] = await Promise.all([
        this.node.getNodeInfo(),
        this.node.getBlockNumber(),
      ]);
      this.#status = {
        ...blank(this.url),
        reachable: true,
        nodeVersion: info.nodeVersion,
        l1ChainId: info.l1ChainId,
        rollupVersion: info.rollupVersion,
        blockNumber: Number(block),
      };
    } catch (error) {
      this.#status = {
        ...blank(this.url),
        error: error instanceof Error ? error.message : String(error),
      };
    }
    return this.#status;
  }

  /** The network label invites carry: `aztec:<l1 chain id>:<rollup version>`. */
  network(): string {
    const { l1ChainId, rollupVersion } = this.#status;
    return `aztec:${l1ChainId ?? "?"}:${rollupVersion ?? "?"}`;
  }
}

function blank(url: string): NodeStatus {
  return {
    url,
    reachable: false,
    nodeVersion: null,
    l1ChainId: null,
    rollupVersion: null,
    blockNumber: null,
    syncedBlock: null,
    checkedAt: Math.floor(Date.now() / 1000),
    error: null,
  };
}
