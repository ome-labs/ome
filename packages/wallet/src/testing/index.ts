/**
 * Test helpers that deal and use viewing shares in-process (through `@ome-labs/wasm/testing`). Never used by the
 * daemon: in production, shares exist only in members' keepers.
 */
import { GrumpkinScalar, type Point } from "@aztec-labs/foundation/curves/grumpkin";
import type { Hex32, MemberPartials } from "@ome-labs/api";
import { type DealtCommittee, dealShares, partials } from "@ome-labs/wasm/testing";

import { fromApiPoint } from "../aztec/convert.ts";
import type { PartialsRequest, PartialsSource } from "../keys/threshold-ecdh.ts";

/** A trusted-dealer viewing committee (tests only; production committees come from the DKG). */
export interface TestCommittee {
  readonly groupKey: Point;
  readonly verificationShares: Point[];
  readonly dealt: DealtCommittee;
}

/** Deals a `threshold`-of-`members` sharing of `secret` (random if absent). */
export function dealTestCommittee(
  threshold: number,
  members: number,
  secret: GrumpkinScalar = GrumpkinScalar.random(),
): TestCommittee {
  const dealt = dealShares(secret.toString(), threshold, members);
  return {
    groupKey: fromApiPoint(dealt.groupKey),
    verificationShares: dealt.verificationShares.map(fromApiPoint),
    dealt,
  };
}

/**
 * Answers ECDH requests with partials from in-process shares, as members' keepers would (each keeper takes the vault's
 * preaddress from its own manifest, never from the request). `members` chooses who responds; `corrupt` members answer
 * with a wrong share, so their proofs fail verification. Records every request.
 */
export class InProcessPartialsSource implements PartialsSource {
  readonly requests: PartialsRequest[] = [];
  readonly #preaddress: Hex32;
  #committee: TestCommittee;
  #members: readonly number[];
  readonly #corrupt: ReadonlySet<number>;

  constructor(
    committee: TestCommittee,
    preaddress: Hex32,
    members: readonly number[],
    options: { corrupt?: readonly number[] } = {},
  ) {
    this.#committee = committee;
    this.#preaddress = preaddress;
    this.#members = members;
    this.#corrupt = new Set(options.corrupt);
  }

  /** Switches to the committee of a new epoch. */
  rotate(committee: TestCommittee, members: readonly number[]): void {
    this.#committee = committee;
    this.#members = members;
  }

  requestPartials(request: PartialsRequest): Promise<MemberPartials[]> {
    this.requests.push(request);
    const { groupKey, shares } = this.#committee.dealt;
    return Promise.resolve(
      this.#members.map((member) => {
        const share = shares[member - 1];
        if (!share) throw new Error(`No share for member ${member}`);
        const secret = this.#corrupt.has(member) ? GrumpkinScalar.random().toString() : share;
        return partials(member, secret, this.#preaddress, groupKey, request.epoch, [
          ...request.points,
        ]);
      }),
    );
  }
}
