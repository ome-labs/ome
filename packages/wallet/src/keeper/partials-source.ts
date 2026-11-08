import type { MemberPartials } from "@ome-labs/api";

import type { PartialsRequest, PartialsSource } from "../keys/threshold-ecdh.ts";
import type { KeeperClient } from "./client.ts";

/**
 * {@link PartialsSource} backed by the member's own keeper: the keeper adds its partials, gathers the rest from other
 * members' keepers and returns `t_v` verified responses. The wallet verifies them again before combining.
 */
export class KeeperPartialsSource implements PartialsSource {
  readonly #client: KeeperClient;

  constructor(client: KeeperClient) {
    this.#client = client;
  }

  async requestPartials(request: PartialsRequest): Promise<MemberPartials[]> {
    const { responses } = await this.#client.call("ecdh.batch", {
      vault: request.vault.toString(),
      epoch: request.epoch,
      purpose: request.purpose,
      points: [...request.points],
    });
    return responses;
  }
}
