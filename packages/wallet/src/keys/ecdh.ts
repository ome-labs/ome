import type { Point } from "@aztec-labs/foundation/curves/grumpkin";
import type { EcdhPurpose } from "@ome-labs/api";

export type { EcdhPurpose };

/** Computes `address_secret · P` for one threshold account. */
export interface EcdhProvider {
  /**
   * Multiplies every point by the account's address secret. Points are on the curve and not the identity; the result
   * has one entry per input, in order. `purpose` is what the keepers record in their audit logs.
   */
  mul(points: readonly Point[], purpose: EcdhPurpose): Promise<Point[]>;
}
