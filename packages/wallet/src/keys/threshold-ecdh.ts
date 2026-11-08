import type { Point } from "@aztec-labs/foundation/curves/grumpkin";
import type { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { EcdhPurpose, Manifest, MemberPartials, Point as ApiPoint } from "@ome-labs/api";
import { verifyAndCombine } from "@ome-labs/wasm";

import { fromApiPoint, toApiPoint } from "../aztec/convert.ts";
import type { EcdhProvider } from "./ecdh.ts";

/** One threshold ECDH request, as a member's keeper answers it (`ecdh.batch`). */
export interface PartialsRequest {
  readonly vault: AztecAddress;
  readonly epoch: number;
  readonly purpose: EcdhPurpose;
  readonly points: readonly ApiPoint[];
}

/** Where partials come from: the member's keeper, or in-process shares in tests. */
export interface PartialsSource {
  /** Responses from at least `t_v` members, each with one partial per point in request order. */
  requestPartials(request: PartialsRequest): Promise<MemberPartials[]>;
}

/**
 * {@link EcdhProvider} for a vault's current committee: asks a {@link PartialsSource} for partials, then verifies every
 * DLEQ proof and combines `t_v` valid partials per point in `@ome-labs/wasm`. The committee comes from the manifest the
 * keeper serves; members whose partials fail are reported to `onInvalidMembers` and ignored.
 */
export class ThresholdEcdhProvider implements EcdhProvider {
  readonly #manifest: Manifest;
  readonly #vault: AztecAddress;
  readonly #source: PartialsSource;
  readonly #onInvalidMembers: ((members: number[]) => void) | undefined;

  constructor(
    vault: AztecAddress,
    manifest: Manifest,
    source: PartialsSource,
    onInvalidMembers?: (members: number[]) => void,
  ) {
    this.#vault = vault;
    this.#manifest = manifest;
    this.#source = source;
    this.#onInvalidMembers = onInvalidMembers;
  }

  async mul(points: readonly Point[], purpose: EcdhPurpose): Promise<Point[]> {
    const inputs = points.map(toApiPoint);
    const { epoch, preaddress, groupKey, viewingThreshold, members } = this.#manifest;
    const responses = await this.#source.requestPartials({
      vault: this.#vault,
      epoch,
      purpose,
      points: inputs,
    });
    const result = verifyAndCombine({
      epoch,
      preaddress,
      ivpk: groupKey,
      verificationShares: members.map((member) => member.verificationShare),
      threshold: viewingThreshold,
      inputs,
      responses,
    });
    if (result.invalidMembers.length > 0) this.#onInvalidMembers?.(result.invalidMembers);
    return result.points.map(fromApiPoint);
  }
}
