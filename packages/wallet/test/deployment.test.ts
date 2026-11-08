// A vault's address, constructor and manifest follow from the setup every founding member agreed on, and match what
// the bindings check proposals against.
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { buildProposal } from "@ome-labs/wasm";
import { describe, expect, it } from "vitest";

import {
  type VaultSetup,
  decodeVaultProtocolSecrets,
  deriveVault,
  encodeVaultProtocolSecrets,
  generateVaultProtocolSecrets,
  toApiPoint,
} from "../src/index.ts";
import { vaultFixture } from "./fixture.ts";

const CHAIN = { chainId: new Fr(31_337n).toString(), version: new Fr(1n).toString() };

describe("deriveVault", () => {
  it("gives every member the same address, matching Aztec and the committee's address context", async () => {
    const { deployment, committee } = await vaultFixture();
    expect(deployment.instance.address).toEqual(deployment.completeAddress.address);
    // A second member deriving from the same setup gets the same vault.
    const again = await deriveVault(deployment.setup);
    expect(again.address).toEqual(deployment.address);
    expect(again.constructorArgs).toEqual(deployment.constructorArgs);
    expect(deployment.constructorArgs).toHaveLength(5);
    expect(deployment.completeAddress.publicKeys.ivpkM).toEqual(committee.groupKey);
  });

  it("produces a manifest and config the bindings accept", async () => {
    const { input, deployment } = await vaultFixture();
    expect(input.manifest.vault).toBe(deployment.address.toString());
    expect(input.manifest.groupKey).toEqual(toApiPoint(deployment.setup.groupKey));
    // An empty proposal (a cancellation) builds only if the manifest and config agree.
    const cancel = {
      calls: [],
      extras: "0x",
      txNonce: Fr.random().toString(),
      expiresAt: 1_900_000_000,
      summary: "cancel",
      proposer: 1,
    };
    expect(buildProposal(input, CHAIN, cancel).id).toMatch(/^0x[0-9a-f]{64}$/u);
    const mismatched = { ...input, config: { ...input.config, threshold: 3 } };
    expect(() => buildProposal(mismatched, CHAIN, cancel)).toThrow();
  });

  it("depends on every setup input and rejects invalid ones", async () => {
    const base: VaultSetup = (await vaultFixture()).deployment.setup;
    const address = (await deriveVault(base)).address;
    expect((await deriveVault({ ...base, salt: Fr.random() })).address).not.toEqual(address);
    expect((await deriveVault({ ...base, threshold: 3 })).address).not.toEqual(address);
    expect(
      (await deriveVault({ ...base, signerKeys: base.signerKeys.toReversed() })).address,
    ).not.toEqual(address);
    await expect(deriveVault({ ...base, threshold: 4 })).rejects.toThrow(/Threshold/);
    await expect(deriveVault({ ...base, viewingThreshold: 1 })).rejects.toThrow(
      /Viewing threshold/,
    );
    await expect(
      deriveVault({ ...base, verificationShares: base.verificationShares.slice(1) }),
    ).rejects.toThrow(/verification share/);
    await expect(deriveVault({ ...base, signerKeys: [] })).rejects.toThrow(/signers/);
  });
});

describe("protocol secrets", () => {
  it("round-trip through the lobby's sealed bytes and refuse anything else", async () => {
    const secrets = await generateVaultProtocolSecrets();
    const bytes = encodeVaultProtocolSecrets(secrets);
    expect(bytes.length).toBeLessThanOrEqual(1024);
    expect(decodeVaultProtocolSecrets(bytes)).toEqual(secrets);
    expect(() => decodeVaultProtocolSecrets(bytes.subarray(1))).toThrow();
    const offCurve = Uint8Array.from(bytes);
    offCurve.set([(bytes.at(-1) ?? 0) ^ 1], bytes.length - 1);
    expect(() => decodeVaultProtocolSecrets(offCurve)).toThrow(/invalid point/);
  });
});
