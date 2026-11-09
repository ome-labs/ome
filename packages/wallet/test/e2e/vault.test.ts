// A 2-of-3 vault with a 2-of-3 viewing committee on a local network: three real `ome-keeper` processes run the DKG,
// two members' wallets read and spend the vault through them, and a fourth keeper joins when member 3 is rotated out.
// No wallet ever holds the vault's incoming viewing secret (the key store throws if asked); every signature comes from
// an `@ome-labs/wasm` signer, which signs only proposals it has checked.
import { Contract } from "@aztec-labs/aztec.js/contracts";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import type { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { ExecutionPayload, mergeExecutionPayloads } from "@aztec-labs/stdlib/tx";
import type { DkgResult, HexBytes, Manifest, Point } from "@ome-labs/api";
import {
  type Rotation,
  UnlockedSigner,
  committeePolynomial,
  decodeProposal,
  generateSigner,
  rotationCall,
} from "@ome-labs/wasm";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import {
  KeeperPartialsSource,
  type VaultDeployment,
  type VaultTx,
  VaultTxError,
  deriveVault,
  fromApiPoint,
  generateVaultProtocolSecrets,
  toVaultRequest,
  vaultArtifact,
  vaultManifest,
} from "../../src/index.ts";
import { type Keeper, launchKeeper, runDkg, runReshare } from "./keepers.ts";
import { type Member, memberWallet, networkAvailable, node } from "./network.ts";

const available = await networkAvailable();

const newSigner = (): UnlockedSigner => {
  const key = `0x${Fr.random().toBuffer().toString("hex")}`;
  return UnlockedSigner.open(generateSigner({ key }), { key });
};

const pad = (points: readonly Point[]) =>
  Array.from({ length: 8 }, (_, i) => {
    const point = points[i];
    return point ? fromApiPoint(point) : { x: Fr.ZERO, y: Fr.ZERO };
  });

/** A deadline `seconds` after the chain's latest block. */
const expiry = async (member: Member, seconds = 3600) =>
  (await member.wallet.chainTime()) + seconds;

/** Polls `check` until it returns a value. */
async function until<T>(check: () => Promise<T | undefined>, what: string): Promise<T> {
  for (let i = 0; i < 600; i++) {
    // oxlint-disable-next-line no-await-in-loop
    const value = await check();
    if (value !== undefined) return value;
    // oxlint-disable-next-line no-await-in-loop
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`timed out waiting for ${what}`);
}

describe.skipIf(!available)("a vault on a local network with real keepers", () => {
  let keepers: Keeper[];
  let alice: Member; // member 1: proposes and executes, through keeper 1
  let bob: Member; // member 2: an independent wallet, through keeper 2
  let signers: UnlockedSigner[];
  let dkg: { session: string; result: DkgResult };
  let deployment: VaultDeployment;
  let vault: AztecAddress;
  let token: TokenContract;

  const balance = async (member: Member, owner: AztecAddress) =>
    (
      await TokenContract.at(token.address, member.wallet)
        .methods.balance_of_private(owner)
        .simulate({ from: owner })
    ).result as bigint;

  /** Member `slot + 1` approves `tx` with its own signer, after checking it in its own wallet. */
  const approve = async (member: Member, tx: VaultTx, slots: number[]): Promise<HexBytes[]> => {
    const input = await member.wallet.vaultInput(vault);
    const chain = await member.wallet.chain();
    const now = await member.wallet.chainTime();
    return slots.map((slot) => {
      const signer = signers[slot];
      if (!signer) throw new Error(`no signer for slot ${slot}`);
      return signer.approve(input, chain, tx.proposal, tx.payloadBody, now);
    });
  };

  const transferFromVault = async (to: AztecAddress, amount: bigint, txNonce?: Fr) =>
    alice.wallet.prepareVaultTx(
      vault,
      await token.methods.transfer(to, amount).request({ fee: alice.fee }),
      { expiresAt: await expiry(alice), summary: `pay ${amount}`, proposer: 1, txNonce },
    );

  beforeAll(async () => {
    const aztecNode = node();
    [alice, bob] = await Promise.all([memberWallet(aztecNode, 0), memberWallet(aztecNode, 1)]);
    keepers = await Promise.all([1, 2, 3, 4].map(() => launchKeeper()));
    const founders = keepers.slice(0, 3);

    dkg = await runDkg(founders, 2);
    signers = [newSigner(), newSigner(), newSigner()];
    deployment = await deriveVault({
      signerKeys: signers.map((s) => fromApiPoint(s.publicKey())),
      threshold: 2,
      viewingThreshold: 2,
      groupKey: fromApiPoint(dkg.result.groupKey),
      verificationShares: dkg.result.verificationShares.map(fromApiPoint),
      secrets: await generateVaultProtocolSecrets(),
      salt: Fr.random(),
    });
    vault = deployment.address;
    const manifest = vaultManifest(
      deployment,
      founders.map((k) => k.contact),
    );
    await Promise.all(
      founders.map((k) => k.client.call("vault.register", { session: dkg.session, manifest })),
    );

    const viewing = (keeper: Keeper) => ({
      manifest,
      transcript: dkg.result.transcript,
      source: new KeeperPartialsSource(keeper.client),
    });
    await alice.wallet.registerVault(deployment, viewing(founders[0] as Keeper));
    await bob.wallet.registerVault(deployment, viewing(founders[1] as Keeper));
    await alice.wallet.deployVault(vault, { from: alice.account, fee: alice.fee });

    ({ contract: token } = await TokenContract.deploy(
      alice.wallet,
      alice.account,
      "Ome",
      "OME",
      18,
    ).send({ from: alice.account, fee: alice.fee }));
    const tokenInstance = await aztecNode.getContract(token.address);
    if (!tokenInstance) throw new Error("Token instance not published");
    await bob.wallet.registerContract(tokenInstance, TokenContract.artifact);
    await token.methods.mint_to_private(vault, 1000n).send({ from: alice.account, fee: alice.fee });
  });

  afterAll(async () => {
    await Promise.all([alice?.wallet.stop(), bob?.wallet.stop()]);
    await Promise.all((keepers ?? []).map((k) => k.stop()));
  });

  it("deploys, and the wallets and keepers agree on the on-chain config", async () => {
    const [config, bobs] = await Promise.all([
      alice.wallet.getVaultConfig(vault),
      bob.wallet.getVaultConfig(vault),
    ]);
    expect(config).toEqual(bobs);
    expect(config).toMatchObject({ threshold: 2, signerCount: 3, viewingThreshold: 2, version: 0 });
    for (const keeper of keepers.slice(0, 3)) {
      // oxlint-disable-next-line no-await-in-loop
      const { observation } = await keeper.client.call("vault.observeConfig", {
        vault: vault.toString(),
        config,
      });
      expect(observation).toBe("current");
    }
  });

  it("finds the vault's notes through threshold ECDH in both wallets", async () => {
    expect(await balance(alice, vault)).toBe(1000n);
    expect(await balance(bob, vault)).toBe(1000n);
  });

  it("previews a vault transfer before any approval", async () => {
    const tx = await transferFromVault(bob.account, 1n);
    const preview = await alice.wallet.previewVaultTx(tx);
    expect(preview.gasUsed.totalGas.l2Gas).toBeGreaterThan(0);
  });

  it("carries a proposal between keepers and pays with two approvals", async () => {
    const [k1, k2] = keepers as [Keeper, Keeper];
    const tx = await transferFromVault(bob.account, 100n);
    await k1.client.call("payloads.put", { vault: vault.toString(), body: tx.payloadBody });
    await k1.client.call("proposals.put", { vault: vault.toString(), body: tx.proposal });

    // Bob's keeper receives the proposal and fetches its payload; Bob rebuilds and checks it in his own wallet.
    const proposal = await until(async () => {
      const bodies = await k2.client.call("proposals.list", { vault: vault.toString() });
      return bodies.find((body) => decodeProposal(body).id === tx.id);
    }, "the proposal at keeper 2");
    const payload = await until(
      async () =>
        (await k2.client.call("payloads.get", {
          vault: vault.toString(),
          hash: decodeProposal(proposal).payloadHash,
        })) ?? undefined,
      "the payload at keeper 2",
    );
    const bobsTx = await bob.wallet.vaultTxOf(vault, proposal, payload);
    expect(bobsTx.outerHash).toEqual(tx.outerHash);
    const [bobsApproval] = await approve(bob, bobsTx, [1]);
    await k2.client.call("approvals.put", { vault: vault.toString(), body: bobsApproval ?? "" });

    const approvals = await until(async () => {
      const list = await k1.client.call("approvals.list", { proposal: tx.id });
      return list.length > 0 ? list : undefined;
    }, "Bob's approval at keeper 1");
    const [alicesApproval] = await approve(alice, tx, [0]);
    const receipt = await alice.wallet.executeVaultTx(tx, [...approvals, alicesApproval ?? ""]);
    expect(receipt.isMined()).toBe(true);
    expect(await balance(alice, vault)).toBe(900n);
    expect(await balance(bob, vault)).toBe(900n);
    expect(await balance(bob, bob.account)).toBe(100n);
  });

  it("refuses to execute below the threshold", async () => {
    const tx = await transferFromVault(bob.account, 1n);
    await expect(alice.wallet.executeVaultTx(tx, await approve(alice, tx, [1]))).rejects.toThrow(
      VaultTxError,
    );
  });

  it("refuses to sign a proposal past its deadline", async () => {
    const tx = await alice.wallet.prepareVaultTx(
      vault,
      await token.methods.transfer(bob.account, 1n).request({ fee: alice.fee }),
      { expiresAt: (await alice.wallet.chainTime()) - 1, summary: "late", proposer: 1 },
    );
    await expect(approve(alice, tx, [0])).rejects.toMatchObject({ kind: "invalid" });
  });

  it("cancels a proposal by burning its nonce", async () => {
    const original = await transferFromVault(bob.account, 5n);
    const originalApprovals = await approve(alice, original, [0, 1]);
    const feeOnly = mergeExecutionPayloads([
      await alice.fee.paymentMethod.getExecutionPayload(),
      ExecutionPayload.empty(),
    ]);
    const cancel = await alice.wallet.prepareVaultTx(vault, feeOnly, {
      expiresAt: await expiry(alice),
      summary: "cancel",
      proposer: 1,
      txNonce: original.txNonce,
    });
    await alice.wallet.executeVaultTx(cancel, await approve(alice, cancel, [1, 2]));
    await expect(alice.wallet.executeVaultTx(original, originalApprovals)).rejects.toThrow(
      /nullifier/i,
    );
    expect(await balance(alice, vault)).toBe(900n);
  });

  it("rotates member 3 out; stale approvals are refused and the new member's count", async () => {
    const [k1, k2, k3, k4] = keepers as [Keeper, Keeper, Keeper, Keeper];
    const stale = await transferFromVault(bob.account, 1n);
    const staleApprovals = await approve(alice, stale, [0, 1]);

    // Members 1 and 2 reshare to themselves and a newcomer, whose keeper is k4; the vault keeps its key.
    const current = (await k1.client.call("vault.status", { vault: vault.toString() })).manifest;
    const newcomer = newSigner();
    const nextKeys = [signers[0], signers[1], newcomer].map((s) =>
      (s as UnlockedSigner).publicKey(),
    );
    const reshare = await runReshare({
      vault: vault.toString(),
      current,
      dealers: [
        { keeper: k1, index: 1 },
        { keeper: k2, index: 2 },
      ],
      next: [k1, k2, k4].map((keeper, i) => ({ keeper, signerKey: nextKeys[i] as Point })),
      threshold: 2,
      viewingThreshold: 2,
    });
    const next = reshare.result.manifest as Manifest;
    const rotation: Rotation = {
      signers: nextKeys,
      threshold: 2,
      viewingThreshold: 2,
      verificationShares: next.members.map((m) => m.verificationShare),
    };

    const rotate = Contract.at(vault, vaultArtifact(), alice.wallet).methods.rotate_membership?.(
      pad(current.members.map((m) => m.signerKey)),
      pad(nextKeys),
      3,
      2,
      2,
      pad(rotation.verificationShares),
      pad(committeePolynomial(2, rotation.verificationShares)),
    );
    if (!rotate) throw new Error("rotate_membership missing from the artifact");
    const payload = await rotate.request({ fee: alice.fee });
    const input = await alice.wallet.vaultInput(vault);
    // The bindings build the same call the artifact encodes.
    expect(toVaultRequest(payload).calls).toContainEqual({
      ...rotationCall(input, rotation),
      signature: null,
    });
    const tx = await alice.wallet.prepareVaultTx(vault, payload, {
      expiresAt: await expiry(alice),
      summary: "member 3 leaves, a newcomer joins",
      proposer: 1,
    });
    const chain = await alice.wallet.chain();
    const now = await alice.wallet.chainTime();
    // Each member trusts the fee payer its own wallet is configured with, never one the proposal names.
    const feePayers = [alice.feePayer.toString()];
    const rotationApprovals = [0, 1].map((slot) =>
      (signers[slot] as UnlockedSigner).approveRotation(
        input,
        chain,
        tx.proposal,
        tx.payloadBody,
        rotation,
        feePayers,
        now,
      ),
    );
    await alice.wallet.executeVaultTx(tx, rotationApprovals);

    // The new committee's keepers activate once they observe it on chain. Member 3's keeper took no part in the
    // reshare, so it cannot verify the new committee: it keeps its share and alarms until its member retires it.
    const config = await bob.wallet.getVaultConfig(vault);
    expect(config.version).toBe(1);
    const observed = await Promise.all(
      [k1, k2, k3, k4].map((k) =>
        k.client.call("vault.observeConfig", { vault: vault.toString(), config }),
      ),
    );
    expect(observed.map((o) => o.observation)).toEqual([
      "activated",
      "activated",
      "unknown",
      "activated",
    ]);
    const retired = await k3.client.call("vault.retire", { vault: vault.toString() });
    expect(retired).toMatchObject({ status: "removed", serving: false });
    await expect(
      k3.client.call("ecdh.batch", {
        vault: vault.toString(),
        epoch: config.version,
        purpose: "decryption",
        points: [current.groupKey],
      }),
    ).rejects.toThrow();

    // Both wallets switch to the new committee and its epoch tagging secret.
    const active = (await k1.client.call("vault.status", { vault: vault.toString() })).manifest;
    expect(active.epoch).toBe(1);
    await alice.wallet.setViewing(vault, {
      manifest: active,
      transcript: reshare.result.transcript ?? "",
      source: new KeeperPartialsSource(k1.client),
    });
    await bob.wallet.setViewing(vault, {
      manifest: active,
      transcript: reshare.result.transcript ?? "",
      source: new KeeperPartialsSource(k2.client),
    });
    signers = [signers[0] as UnlockedSigner, signers[1] as UnlockedSigner, newcomer];

    await expect(alice.wallet.executeVaultTx(stale, staleApprovals)).rejects.toMatchObject({
      reason: "stale-config",
    });

    const fresh = await transferFromVault(bob.account, 40n);
    await alice.wallet.executeVaultTx(fresh, await approve(alice, fresh, [0, 2]));
    expect(await balance(alice, vault)).toBe(860n);
    expect(await balance(bob, vault)).toBe(860n);
  });
});
