// A 2-of-3 vault with a 2-of-3 viewing committee on a local network: three real `ome-keeper` processes run the DKG,
// two members' wallets read and spend the vault through them (a swap through Aztec's AMM included, under an authwit
// the members approve with it), and a fourth keeper joins when member 3 is rotated out.
// No wallet ever holds the vault's incoming viewing secret (the key store throws if asked); every signature comes from
// an `@ome-labs/wasm` signer, which signs only proposals it has checked.
import { Contract } from "@aztec-labs/aztec.js/contracts";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { AMMContract } from "@aztec-labs/noir-contracts.js/AMM";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import { loadContractArtifact } from "@aztec-labs/stdlib/abi";
import type { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type { NoirCompiledContract } from "@aztec-labs/stdlib/noir";
import { ExecutionPayload, mergeExecutionPayloads } from "@aztec-labs/stdlib/tx";
import type { DkgResult, HexBytes, Manifest, Point } from "@ome-labs/api";
import compiledRouter from "@ome-labs/contracts/TestRouter.json" with { type: "json" };
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
      return list.length > 0 ? list.map((vote) => vote.body) : undefined;
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
    await expect(approve(alice, tx, [0])).rejects.toMatchObject({ kind: "expired" });
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

  it("swaps through Aztec's AMM under a private authwit the members approve with the swap", async () => {
    const send = { from: alice.account, fee: alice.fee };
    const deployToken = async (name: string, symbol: string) =>
      (await TokenContract.deploy(alice.wallet, alice.account, name, symbol, 18).send(send))
        .contract;
    const opal = await deployToken("Opal", "OPAL");
    const shares = await deployToken("Ome LP", "OLP");
    const { contract: amm } = await AMMContract.deploy(
      alice.wallet,
      token.address,
      opal.address,
      shares.address,
    ).send(send);
    await shares.methods.set_minter(amm.address, true).send(send);

    // Alice provides the liquidity from her own account, under her own authwits.
    await token.methods.mint_to_private(alice.account, 10_000n).send(send);
    await opal.methods.mint_to_private(alice.account, 10_000n).send(send);
    const liquidityNonce = Fr.random();
    const authWitnesses = await Promise.all(
      [token, opal].map(async (t) =>
        alice.wallet.createAuthWit(alice.account, {
          caller: amm.address,
          call: await t.methods
            .transfer_to_public_and_prepare_private_balance_increase(
              alice.account,
              amm.address,
              10_000n,
              liquidityNonce,
            )
            .getFunctionCall(),
        }),
      ),
    );
    await amm.methods
      .add_liquidity(10_000n, 10_000n, 9_000n, 9_000n, liquidityNonce)
      .with({ authWitnesses })
      .send(send);

    // The vault swaps 100 OME for OPAL; the AMM pulls the OME under an authwit granted with the swap.
    const before = await balance(alice, vault);
    const nonce = Fr.random();
    const tx = await alice.wallet.prepareVaultTx(
      vault,
      await amm.methods
        .swap_exact_tokens_for_tokens(token.address, opal.address, 100n, 1n, nonce)
        .request({ fee: alice.fee }),
      {
        expiresAt: await expiry(alice),
        summary: "swap 100 OME for OPAL",
        proposer: 1,
        authwits: [
          {
            caller: amm.address,
            call: await token.methods
              .transfer_to_public(vault, amm.address, 100n, nonce)
              .getFunctionCall(),
          },
        ],
      },
    );
    expect(tx.authwitHashes).toHaveLength(1);
    expect(tx.approvalMessages).toHaveLength(2);

    // Bob rebuilds the proposal in his own wallet, authwit included, and both sign the swap and the authwit.
    const bobsTx = await bob.wallet.vaultTxOf(vault, tx.proposal, tx.payloadBody);
    expect(bobsTx.authwitHashes).toEqual(tx.authwitHashes);
    const approvals = [...(await approve(bob, bobsTx, [1])), ...(await approve(alice, tx, [0]))];
    const receipt = await alice.wallet.executeVaultTx(tx, approvals);
    expect(receipt.isMined()).toBe(true);

    expect(await balance(alice, vault)).toBe(before - 100n);
    const opals = await until(async () => {
      const { result } = await TokenContract.at(opal.address, alice.wallet)
        .methods.balance_of_private(vault)
        .simulate({ from: vault });
      return (result as bigint) > 0n ? (result as bigint) : undefined;
    }, "the vault's OPAL");
    expect(opals).toBeGreaterThan(0n);
  });

  it("grants an authwit to a contract the vault reaches through a router, and refuses one it never uses", async () => {
    const send = { from: alice.account, fee: alice.fee };
    const artifact = loadContractArtifact(compiledRouter as unknown as NoirCompiledContract);
    const deployRouter = async () =>
      (await Contract.deploy(alice.wallet, artifact, []).send(send)).contract;
    const router = await deployRouter();
    const puller = await deployRouter();
    const nonce = Fr.random();
    const pull = (amount: bigint) =>
      token.methods.transfer_to_public(vault, puller.address, amount, nonce).getFunctionCall();
    const route = await router.methods.route?.(puller.address, token.address, 10n, nonce);
    if (!route) throw new Error("route missing from the artifact");
    const proposeRoute = async (amount: bigint) =>
      alice.wallet.prepareVaultTx(vault, await route.request({ fee: alice.fee }), {
        expiresAt: await expiry(alice),
        summary: "route 10 OME",
        proposer: 1,
        authwits: [{ caller: puller.address, call: await pull(amount) }],
      });
    const usage = async (tx: VaultTx) =>
      alice.wallet.unusedAuthwits(
        tx,
        await alice.wallet.simulationFacts(tx, await alice.wallet.previewVaultTx(tx)),
      );

    // The puller is not one of the vault's calls: the router reaches it. The simulation shows the authwit used.
    const routed = await proposeRoute(10n);
    expect(await usage(routed)).toEqual([]);
    // An authwit for another amount is one the transaction never asks about.
    expect(await usage(await proposeRoute(11n))).toEqual([0]);

    const before = await balance(alice, vault);
    const approvals = await approve(alice, routed, [0, 1]);
    expect((await alice.wallet.executeVaultTx(routed, approvals)).isMined()).toBe(true);
    expect(await balance(alice, vault)).toBe(before - 10n);
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

    const before = await balance(alice, vault);
    const fresh = await transferFromVault(bob.account, 40n);
    await alice.wallet.executeVaultTx(fresh, await approve(alice, fresh, [0, 2]));
    expect(await balance(alice, vault)).toBe(before - 40n);
    expect(await balance(bob, vault)).toBe(before - 40n);
  });
});
