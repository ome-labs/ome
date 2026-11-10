import { computeAuthWitMessageHash } from "@aztec-labs/aztec.js/authorization";
// A vault transaction is signed as an Ome proposal but executed by Aztec's account entrypoint, so both must hash the
// payload alike. Approvals tally into the 49-field witness in slot order, which the vault's witness provider serves
// only for the approved hash.
import {
  AccountFeePaymentMethodOptions,
  DefaultAccountEntrypoint,
} from "@aztec-labs/entrypoints/account";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { FunctionCall, FunctionSelector, FunctionType } from "@aztec-labs/stdlib/abi";
import { AuthWitness } from "@aztec-labs/stdlib/auth-witness";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { GasSettings } from "@aztec-labs/stdlib/gas";
import { ExecutionPayload } from "@aztec-labs/stdlib/tx";
import { buildProposal, checkProposal, tally } from "@ome-labs/wasm";
import { describe, expect, it } from "vitest";

import {
  VaultWitnessProvider,
  fromVaultRequest,
  toVaultRequest,
  vaultAuthwits,
} from "../src/index.ts";
import { vaultFixture } from "./fixture.ts";

const chainInfo = { chainId: new Fr(31_337n), version: new Fr(1n) };
const chain = { chainId: chainInfo.chainId.toString(), version: chainInfo.version.toString() };
const EXPIRES_AT = 1_900_000_000;
const NOW = EXPIRES_AT - 100;

async function payload(): Promise<ExecutionPayload> {
  const transfer = new FunctionCall(
    "transfer_private_to_private",
    AztecAddress.fromFieldUnsafe(Fr.random()),
    await FunctionSelector.fromSignature("transfer_private_to_private((Field),(Field),u128,Field)"),
    FunctionType.PRIVATE,
    false,
    false,
    [Fr.random(), Fr.random(), new Fr(25n), Fr.random()],
  );
  const sponsor = new FunctionCall(
    "sponsor_unconditionally",
    AztecAddress.fromFieldUnsafe(Fr.random()),
    await FunctionSelector.fromSignature("sponsor_unconditionally()"),
    FunctionType.PRIVATE,
    false,
    false,
    [],
  );
  return new ExecutionPayload(
    [transfer, sponsor],
    [new AuthWitness(Fr.random(), [Fr.random()])],
    [],
    [],
    AztecAddress.fromFieldUnsafe(Fr.random()),
  );
}

/** The hash Aztec's default account entrypoint asks the vault to authorize for `exec`. */
async function aztecOuterHash(
  vault: AztecAddress,
  exec: ExecutionPayload,
  txNonce: Fr,
): Promise<Fr> {
  let captured: Fr | undefined;
  const capture = {
    createAuthWit: (messageHash: Fr) => {
      captured = messageHash;
      return Promise.resolve(new AuthWitness(messageHash, []));
    },
  };
  await new DefaultAccountEntrypoint(vault, capture).createTxExecutionRequest(
    exec,
    GasSettings.empty(),
    chainInfo,
    {
      txNonce,
      cancellable: true,
      feePaymentMethodOptions: AccountFeePaymentMethodOptions.EXTERNAL,
    },
  );
  if (!captured) throw new Error("no witness requested");
  return captured;
}

describe("vault proposals", () => {
  it("hash the payload as Aztec's entrypoint does, and round-trip it", async () => {
    const { input, deployment } = await vaultFixture();
    const exec = await payload();
    const txNonce = Fr.random();
    const built = buildProposal(input, chain, {
      ...toVaultRequest(exec),
      txNonce: txNonce.toString(),
      createdAt: EXPIRES_AT - 3600,
      expiresAt: EXPIRES_AT,
      summary: "pay 25",
      proposer: 1,
    });
    expect(built.outerHash).toBe(
      (await aztecOuterHash(deployment.address, exec, txNonce)).toString(),
    );

    const rebuilt = fromVaultRequest(
      checkProposal(input, chain, built.proposal, built.payload, NOW),
    );
    expect(toVaultRequest(rebuilt)).toEqual(toVaultRequest(exec));
    expect(rebuilt.feePayer).toEqual(exec.feePayer);
    expect(rebuilt.authWitnesses).toEqual(exec.authWitnesses);
  });

  it("hash authwits as aztec.js does, and round-trip them", async () => {
    const { input } = await vaultFixture();
    const exec = await payload();
    const [transfer] = exec.calls;
    if (!transfer) throw new Error("no call");
    // The first call's target may pull another of the vault's tokens.
    const authwit = {
      caller: transfer.to,
      call: new FunctionCall(
        "",
        AztecAddress.fromFieldUnsafe(Fr.random()),
        await FunctionSelector.fromSignature("transfer_to_public((Field),(Field),u128,Field)"),
        FunctionType.PRIVATE,
        false,
        false,
        [Fr.random(), Fr.random(), new Fr(9n), Fr.random()],
      ),
    };
    const built = buildProposal(input, chain, {
      ...toVaultRequest(exec, undefined, [authwit]),
      txNonce: Fr.random().toString(),
      createdAt: EXPIRES_AT - 3600,
      expiresAt: EXPIRES_AT,
      summary: "swap",
      proposer: 1,
    });
    expect(built.authwits).toEqual([
      (await computeAuthWitMessageHash(authwit, chainInfo)).toString(),
    ]);
    const checked = checkProposal(input, chain, built.proposal, built.payload, NOW);
    expect(vaultAuthwits(checked)).toEqual([authwit]);
  });

  it("refuses a utility call, which the entrypoint cannot run", async () => {
    const exec = await payload();
    const [call] = exec.calls;
    if (!call) throw new Error("no call");
    call.type = FunctionType.UTILITY;
    expect(() => toVaultRequest(exec)).toThrow(/utility/);
  });
});

describe("approvals", () => {
  it("tally into the witness in slot order with the empty slot zeroed", async () => {
    const { input, signers } = await vaultFixture();
    const built = buildProposal(input, chain, {
      ...toVaultRequest(await payload()),
      txNonce: Fr.random().toString(),
      createdAt: EXPIRES_AT - 3600,
      expiresAt: EXPIRES_AT,
      summary: "pay 25",
      proposer: 2,
    });
    const approve = (slot: number) =>
      signers[slot]?.approve(input, chain, built.proposal, built.payload, NOW) ?? "";

    const one = tally(input, built.proposal, [approve(2)], [], NOW);
    expect(one).toMatchObject({ status: "pending", approvals: 1, needed: 2, witness: null });

    const { status, witness } = tally(input, built.proposal, [approve(2), approve(0)], [], NOW);
    expect(status).toBe("approved");
    const fields = (witness ?? []).map((field) => Fr.fromString(field));
    expect(fields).toHaveLength(49);
    expect(fields.slice(16, 20).some((f) => !f.isZero())).toBe(true);
    expect(fields.slice(20, 24).every((f) => f.isZero())).toBe(true);
    expect(fields.slice(24, 28).some((f) => !f.isZero())).toBe(true);
    expect(fields[48]).toEqual(new Fr(BigInt(EXPIRES_AT)));
  });

  it("are served only for the hash they approve, until revoked", async () => {
    const provider = new VaultWitnessProvider();
    const hash = Fr.random();
    const fields = [Fr.random(), Fr.random()];
    await expect(provider.createAuthWit(hash)).rejects.toThrow(/No approvals/);
    provider.provide(hash, fields);
    expect((await provider.createAuthWit(hash)).witness).toEqual(fields);
    await expect(provider.createAuthWit(Fr.random())).rejects.toThrow(/No approvals/);
    provider.revoke(hash);
    await expect(provider.createAuthWit(hash)).rejects.toThrow(/No approvals/);
  });
});
