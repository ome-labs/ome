import {
  type Authwit,
  type Call,
  type OmeWasmError,
  UnlockedSigner,
  addressContext,
  addressPublicShares,
  approvalMessage,
  authwitHash,
  buildProposal,
  checkProposal,
  checkRotation,
  decodeApproval,
  decodeProposal,
  decodeRejection,
  epochTaggingSecret,
  generateSigner,
  outerHash,
  policyWarnings,
  rotationCall,
  sealedPublicKey,
  sealingKey,
  selector,
  signerSwapSlot,
  tally,
  verifyAndCombine,
  verifyApproval,
  verifyRejection,
  committeePolynomial,
  verifyRotationShares,
} from "@ome-labs/wasm";
import { dealShares, partials } from "@ome-labs/wasm/testing";
// Every exported area works from Node through the published package layout, and failures arrive as OmeWasmError
// with an API error kind. The Rust logic itself is tested in Rust; these tests cover the boundary.
import { describe, expect, it } from "vitest";

import { at, chain, fixture, randomField } from "./fixture.ts";

const NOW = 1_800_000_000;

/** Runs `run` and returns what it threw. */
function thrown(run: () => unknown): OmeWasmError {
  try {
    run();
  } catch (error) {
    return error as OmeWasmError;
  }
  throw new Error("expected a throw");
}

const transfer = (): Call => ({
  signature: "transfer_private_to_private((Field),(Field),u128,Field)",
  to: randomField(),
  selector: selector("transfer_private_to_private((Field),(Field),u128,Field)"),
  kind: "private",
  isStatic: false,
  hideMsgSender: false,
  args: [randomField(), randomField(), `0x${"00".repeat(31)}19`, randomField()],
});

function proposal(
  f: ReturnType<typeof fixture>,
  expiresAt = NOW + 3600,
  calls: Call[] = [transfer()],
  authwits: Authwit[] = [],
) {
  return buildProposal(f.vault, chain, {
    calls,
    authwits,
    extras: "0x",
    txNonce: randomField(),
    createdAt: Math.min(NOW, expiresAt),
    expiresAt,
    summary: "pay the auditors",
    proposer: 1,
  });
}

describe("threshold reading", () => {
  it("combines any t_v members' partials to the same points and names a bad member", () => {
    const f = fixture();
    const { preaddress, groupKey } = f.vault.manifest;
    const inputs = [addressContext(preaddress, groupKey).addressPoint, groupKey];
    const responses = f.committee.shares.map((share, i) =>
      partials(i + 1, share, preaddress, groupKey, 0, inputs),
    );
    const request = {
      epoch: 0,
      preaddress,
      ivpk: groupKey,
      verificationShares: f.committee.verificationShares,
      threshold: 2,
      inputs,
    };
    const first = verifyAndCombine({ ...request, responses: [at(responses, 0), at(responses, 1)] });
    const last = verifyAndCombine({ ...request, responses: [at(responses, 1), at(responses, 2)] });
    expect(last.points).toEqual(first.points);
    expect(first.invalidMembers).toEqual([]);

    const forged = { ...at(responses, 0), partials: [...at(responses, 2).partials] };
    const withForgery = verifyAndCombine({
      ...request,
      responses: [forged, ...responses.slice(1)],
    });
    expect(withForgery.points).toEqual(first.points);
    expect(withForgery.invalidMembers).toEqual([1]);

    const short = thrown(() =>
      verifyAndCombine({ ...request, responses: [forged, at(responses, 1)] }),
    );
    expect(short.name).toBe("OmeWasmError");
    expect(short.kind).toBe("invalid");

    // A wrong epoch binds the proofs to another context, so every member is refused.
    expect(thrown(() => verifyAndCombine({ ...request, epoch: 1, responses })).kind).toBe(
      "invalid",
    );
  });

  it("derives each member's public address share", () => {
    const f = fixture();
    const shares = addressPublicShares(
      f.vault.manifest.preaddress,
      f.vault.manifest.groupKey,
      f.committee.verificationShares,
    );
    expect(shares).toHaveLength(3);
    expect(new Set(shares.map((s) => s.x)).size).toBe(3);
  });
});

describe("proposals and votes", () => {
  it("builds, checks, approves, rejects and tallies into the witness", () => {
    const f = fixture();
    const built = proposal(f);
    const info = decodeProposal(built.proposal);
    expect(info.id).toBe(built.id);
    expect(info.feeMethod).toBe(0);
    expect(info.summary).toBe("pay the auditors");
    expect(built.approvalMessages).toEqual([
      approvalMessage(f.vault.manifest.vault, built.outerHash, 0, info.expiresAt),
    ]);

    const request = checkProposal(f.vault, chain, built.proposal, built.payload, NOW);
    expect(request.calls).toHaveLength(1);
    expect(outerHash(f.vault.manifest.vault, chain, info.txNonce, request.calls)).toBe(
      built.outerHash,
    );

    const first = at(f.signers, 0).approve(f.vault, chain, built.proposal, built.payload, NOW);
    expect(decodeApproval(first)).toMatchObject({ proposal: built.id, slot: 0 });
    verifyApproval(f.vault, built.proposal, first);
    expect(tally(f.vault, built.proposal, [first], [], NOW)).toMatchObject({
      status: "pending",
      approvals: 1,
      needed: 2,
      witness: null,
    });

    const rejection = at(f.signers, 2).reject(f.vault, built.proposal, "too much");
    expect(decodeRejection(rejection)).toMatchObject({ slot: 2, reason: "too much" });
    verifyRejection(f.vault, built.proposal, rejection);

    const second = at(f.signers, 1).approve(f.vault, chain, built.proposal, built.payload, NOW);
    const other = proposal(f);
    const foreign = at(f.signers, 2).approve(f.vault, chain, other.proposal, other.payload, NOW);
    const result = tally(
      f.vault,
      built.proposal,
      [first, first, foreign, second],
      [rejection],
      NOW,
    );
    expect(result.status).toBe("approved");
    expect(result.approvals).toBe(2);
    expect(result.rejections).toBe(1);
    expect(result.refused).toEqual([expect.objectContaining({ index: 2, approval: true })]);
    expect(result.witness).toHaveLength(49);
    expect(tally(f.vault, built.proposal, [first], [], NOW + 7200).status).toBe("expired");
  });

  it("signs and tallies the authwits a proposal grants", () => {
    const f = fixture();
    const amm = transfer();
    const authwit: Authwit = { caller: amm.to, call: { ...transfer(), to: randomField() } };
    const built = proposal(f, NOW + 3600, [amm], [authwit]);
    const messageHash = authwitHash(authwit, chain);
    expect(built.authwits).toEqual([messageHash]);
    expect(built.approvalMessages).toEqual([
      approvalMessage(f.vault.manifest.vault, built.outerHash, 0, NOW + 3600),
      approvalMessage(f.vault.manifest.vault, messageHash, 0, NOW + 3600),
    ]);
    expect(checkProposal(f.vault, chain, built.proposal, built.payload, NOW).authwits).toEqual([
      authwit,
    ]);

    const approvals = [0, 1].map((slot) =>
      at(f.signers, slot).approve(f.vault, chain, built.proposal, built.payload, NOW),
    );
    expect(decodeApproval(at(approvals, 0)).signatures).toHaveLength(2);
    const result = tally(f.vault, built.proposal, approvals, [], NOW);
    expect(result.status).toBe("approved");
    expect(result.authwitWitnesses).toEqual([
      { messageHash, witness: expect.any(Array) as unknown },
    ]);
    expect(result.authwitWitnesses?.[0]?.witness).toHaveLength(49);

    // A caller reached through another contract is allowed (the daemon's simulation checks the use); a transaction
    // without calls uses no authwit.
    expect(
      proposal(f, NOW + 3600, [amm], [{ ...authwit, caller: randomField() }]).authwits,
    ).toHaveLength(1);
    expect(thrown(() => proposal(f, NOW + 3600, [], [authwit])).message).toMatch(
      /without calls uses no authwit/u,
    );
  });

  it("refuses to sign what fails the checks", () => {
    const f = fixture();
    const built = proposal(f, NOW - 1);
    expect(
      thrown(() => checkProposal(f.vault, chain, built.proposal, built.payload, NOW)).kind,
    ).toBe("expired");
    expect(
      thrown(() => at(f.signers, 0).approve(f.vault, chain, built.proposal, built.payload, NOW))
        .kind,
    ).toBe("expired");
    const fresh = proposal(f);
    const tampered = proposal(f);
    expect(
      thrown(() => checkProposal(f.vault, chain, fresh.proposal, tampered.payload, NOW)).message,
    ).toMatch(/payload/u);
    expect(thrown(() => verifyApproval(f.vault, fresh.proposal, "0x00")).kind).toBe("invalid");
  });
});

describe("signer", () => {
  it("seals under a passphrase, opens with it or its sealing key, and refuses a wrong one", () => {
    const sealed = generateSigner({ passphrase: "correct horse battery" });
    expect(JSON.parse(sealed)).toMatchObject({ v: expect.any(String) });
    const signer = UnlockedSigner.open(sealed, { passphrase: "correct horse battery" });
    expect(signer.publicKey()).toEqual(sealedPublicKey(sealed));
    const key = sealingKey(sealed, "correct horse battery");
    const again = UnlockedSigner.open(sealed, { key });
    expect(again.publicKey()).toEqual(signer.publicKey());
    signer.free();
    again.free();
    expect(thrown(() => UnlockedSigner.open(sealed, { passphrase: "wrong" })).kind).toBe("auth");
  });
});

describe("membership", () => {
  it("checks a reshared committee and builds its rotate_membership call", () => {
    const f = fixture();
    const reshared = dealShares(f.secret, 2, 3);
    const rotation = {
      signers: f.signers.map((s) => s.publicKey()),
      threshold: 2,
      viewingThreshold: 2,
      verificationShares: reshared.verificationShares,
    };
    verifyRotationShares(f.vault.manifest.groupKey, 2, reshared.verificationShares);
    expect(checkRotation(f.vault, rotation).version).toBe(1);
    const call = rotationCall(f.vault, rotation);
    expect(call.to).toBe(f.vault.manifest.vault);
    expect(call.selector).toBe(0xe1071b7c);
    // The committee polynomial goes with the call: it starts at the vault's viewing key.
    const polynomial = committeePolynomial(2, reshared.verificationShares);
    expect(polynomial).toHaveLength(2);
    expect(polynomial[0]).toEqual(f.vault.manifest.groupKey);

    const stranger = dealShares(randomField(), 2, 3);
    expect(
      thrown(() => verifyRotationShares(f.vault.manifest.groupKey, 2, stranger.verificationShares))
        .kind,
    ).toBe("invalid");
    // The same shares are not a reshare.
    expect(
      thrown(() =>
        checkRotation(f.vault, { ...rotation, verificationShares: f.committee.verificationShares }),
      ).kind,
    ).toBe("invalid");
  });

  it("recognises and approves a signer swap, and only a swap", () => {
    const f = fixture();
    const key = randomField();
    const phone = UnlockedSigner.open(generateSigner({ key }), { key });
    const keys = f.signers.map((s) => s.publicKey());
    const swap = {
      signers: keys.map((k, i) => (i === 2 ? phone.publicKey() : k)),
      threshold: 2,
      viewingThreshold: 2,
      verificationShares: f.committee.verificationShares,
    };
    const built = buildProposal(f.vault, chain, {
      calls: [rotationCall(f.vault, swap)],
      authwits: [],
      extras: "0x",
      txNonce: randomField(),
      createdAt: NOW,
      expiresAt: NOW + 3600,
      summary: "move member 3 to a phone",
      proposer: 1,
    });
    expect(signerSwapSlot(f.vault, built.payload)).toBe(2);
    const approval = at(f.signers, 0).approveSignerSwap(
      f.vault,
      chain,
      built.proposal,
      built.payload,
      [],
      NOW,
    );
    verifyApproval(f.vault, built.proposal, approval);
    expect(signerSwapSlot(f.vault, proposal(f).payload)).toBeNull();
    expect(
      thrown(() =>
        at(f.signers, 0).approveSignerSwap(
          f.vault,
          chain,
          proposal(f).proposal,
          proposal(f).payload,
          [],
          NOW,
        ),
      ).kind,
    ).toBe("invalid");
    phone.free();
  });

  it("derives a different self-send tagging secret per epoch", () => {
    const vault = randomField();
    const transcript = `0x${"07".repeat(64)}`;
    const first = epochTaggingSecret(vault, 1, transcript);
    expect(epochTaggingSecret(vault, 1, transcript)).toEqual(first);
    expect(epochTaggingSecret(vault, 2, transcript)).not.toEqual(first);
    expect(thrown(() => epochTaggingSecret(vault, 1, "0x07")).kind).toBe("invalid");
  });

  it("warns about risky thresholds", () => {
    expect(policyWarnings(1, 3, 3)).toEqual(["single-signer", "no-spare-viewer"]);
    expect(policyWarnings(2, 2, 3)).toEqual(["two-can-read"]);
    expect(policyWarnings(2, 3, 4)).toEqual([]);
  });
});

describe("errors", () => {
  it("throws OmeWasmError with a kind for malformed input", () => {
    const error = thrown(() => addressContext("0x12", { x: "0x01", y: "0x02" }));
    expect(error).toBeInstanceOf(Error);
    expect(error.name).toBe("OmeWasmError");
    expect(error.kind).toBe("invalid");
    expect(thrown(() => decodeProposal("0x00")).kind).toBe("invalid");
    expect(thrown(() => approvalMessage(randomField(), randomField(), 0, 1.5)).kind).toBe(
      "invalid",
    );
  });
});
