// A member reviews a proposal rebuilt on their own device: when the rebuilt payload does not hash to the proposal's
// outer hash, the review says so, and the daemon refuses to approve it.
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { VaultTxError } from "@ome-labs/wallet";
import { buildProposal, decodeProposal } from "@ome-labs/wasm";
import { describe, expect, it } from "vitest";

import { CHAIN, NOW, fakeContext } from "../../test/fake-context.ts";
import { ProposalService } from "./service.ts";

describe("proposals.review", () => {
  it("reports a payload that rebuilds to another outer hash, and approval refuses it", async () => {
    const fake = await fakeContext();
    const built = buildProposal({ manifest: fake.manifest, config: fake.config }, CHAIN, {
      calls: [],
      authwits: [],
      extras: "0x",
      txNonce: Fr.random().toString(),
      createdAt: NOW,
      expiresAt: NOW + 3600,
      summary: JSON.stringify({ kind: "cancel", cancels: `0x${"00".repeat(32)}`, memo: null }),
      proposer: 1,
    });
    fake.stored.proposals.push(built.proposal);
    fake.stored.payloads.set(decodeProposal(built.proposal).payloadHash, built.payload);
    fake.vaultTxOf = () =>
      Promise.reject(
        new VaultTxError(
          "outer-hash",
          "Aztec's entrypoint hashes the payload differently from the proposal",
        ),
      );

    const service = new ProposalService(fake.context);
    const ref = { vault: fake.deployment.address.toString(), id: built.id };
    const review = await service.review(ref);
    expect(review.checks.rebuilt).toBe(false);
    expect(review.checks.failure).toEqual({
      failure: { kind: "payloadMismatch" },
      message: "Aztec's entrypoint hashes the payload differently from the proposal",
    });
    expect(review.simulation.ok).toBe(false);
    await expect(service.approve(ref)).rejects.toThrow(VaultTxError);
  });
});
