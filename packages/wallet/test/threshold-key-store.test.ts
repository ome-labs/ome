import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { GrumpkinScalar, Point } from "@aztec-labs/foundation/curves/grumpkin";
import { openTmpStore } from "@aztec-labs/kv-store/lmdb-v2";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { deriveKeys, derivePublicKeyFromSecretKey, hashPublicKey } from "@aztec-labs/stdlib/keys";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  type EcdhProvider,
  type EcdhPurpose,
  ThresholdKeyStore,
  ThresholdViewingError,
  generateVaultProtocolSecrets,
  thresholdAccountKeys,
} from "../src/index.ts";

/** Multiplies by a known scalar and records every call. */
class CountingProvider implements EcdhProvider {
  readonly calls: { points: number; purpose: EcdhPurpose }[] = [];
  readonly secret: GrumpkinScalar;

  constructor(secret: GrumpkinScalar) {
    this.secret = secret;
  }

  async mul(points: readonly Point[], purpose: EcdhPurpose): Promise<Point[]> {
    this.calls.push({ points: points.length, purpose });
    const { Grumpkin } = await import("@aztec-labs/foundation/crypto/grumpkin");
    return Promise.all(points.map((p) => Grumpkin.mul(p, this.secret)));
  }
}

describe("ThresholdKeyStore", () => {
  let store: Awaited<ReturnType<typeof openTmpStore>>;
  let keyStore: ThresholdKeyStore;
  let vault: AztecAddress;
  let ivpk: Point;

  beforeEach(async () => {
    store = await openTmpStore("key-store-test", true);
    keyStore = new ThresholdKeyStore(store);
    ivpk = await derivePublicKeyFromSecretKey(GrumpkinScalar.random());
    const keys = thresholdAccountKeys(await generateVaultProtocolSecrets(), ivpk);
    vault = (await keyStore.addAccount(keys, Fr.random())).address;
  });

  afterEach(async () => {
    await store.delete();
  });

  it("registers a threshold account without an incoming viewing secret", async () => {
    expect(await keyStore.isThresholdAccount(vault)).toBe(true);
    expect(await keyStore.hasAccount(vault)).toBe(true);
    expect((await keyStore.getAccounts()).map(String)).toContain(vault.toString());
    expect(await keyStore.getMasterIncomingViewingPublicKey(vault)).toEqual(ivpk);
  });

  it("refuses every read of the incoming viewing secret", async () => {
    await expect(keyStore.getMasterIncomingViewingSecretKey(vault)).rejects.toThrow(
      ThresholdViewingError,
    );
    await expect(keyStore.getAccountSecretKeys(vault)).rejects.toThrow(ThresholdViewingError);
    await expect(keyStore.getMasterSecretKey(await hashPublicKey(ivpk))).rejects.toThrow(
      ThresholdViewingError,
    );
  });

  it("still serves the other secrets (nullifiers, outgoing viewing)", async () => {
    const app = new AztecAddress(new Fr(0x1234n));
    await expect(keyStore.getAppOutgoingViewingSecretKey(vault, app)).resolves.toBeInstanceOf(Fr);
    const npk = await keyStore.getMasterNullifierHidingPublicKey(vault);
    await expect(
      keyStore.getKeyValidationRequest(await hashPublicKey(npk), app),
    ).resolves.toBeDefined();
  });

  it("keeps ordinary accounts working alongside", async () => {
    const keys = await deriveKeys(Fr.random());
    const { address } = await keyStore.addAccount(
      {
        ...keys,
        masterMessageSigningPublicKey: keys.masterMessageSigningPublicKey,
        masterFallbackPublicKey: keys.masterFallbackPublicKey,
      },
      Fr.random(),
    );
    expect(await keyStore.isThresholdAccount(address)).toBe(false);
    expect(await keyStore.getMasterIncomingViewingSecretKey(address)).toEqual(
      keys.masterIncomingViewingSecretKey,
    );
    expect((await keyStore.getAccounts()).length).toBe(2);
  });

  it("batches concurrent requests, de-duplicates and caches per purpose", async () => {
    const provider = new CountingProvider(GrumpkinScalar.random());
    keyStore.setEcdhProvider(vault, provider);
    const points = await Promise.all(
      [1, 2, 3].map(() => derivePublicKeyFromSecretKey(GrumpkinScalar.random())),
    );
    const [a, b] = points as [Point, Point, Point];

    // One provider call for concurrent requests; a duplicate point in flight is requested once.
    const results = await Promise.all(
      [...points, a].map((p) => keyStore.addressSecretMul(vault, [p])),
    );
    expect(provider.calls).toEqual([{ points: 3, purpose: "decryption" }]);
    const { Grumpkin } = await import("@aztec-labs/foundation/crypto/grumpkin");
    const expected = await Promise.all([...points, a].map((p) => Grumpkin.mul(p, provider.secret)));
    expect(results.flat()).toEqual(expected);

    // Repeats are served from memory, separately for each purpose the keepers audit.
    await keyStore.addressSecretMul(vault, [a, b]);
    await keyStore.addressSecretMul(vault, [b], { purpose: "tagging" });
    await keyStore.addressSecretMul(vault, [b], { purpose: "tagging" });
    await keyStore.addressSecretMul(vault, [b], { purpose: "handshake" });
    expect(provider.calls).toEqual([
      { points: 3, purpose: "decryption" },
      { points: 1, purpose: "tagging" },
      { points: 1, purpose: "handshake" },
    ]);

    // A new committee epoch drops the remembered results.
    keyStore.setEcdhProvider(vault, provider);
    await keyStore.addressSecretMul(vault, [a]);
    expect(provider.calls).toHaveLength(4);
  });

  it("maps off-curve inputs to unrelated points and rejects the identity", async () => {
    const provider = new CountingProvider(GrumpkinScalar.random());
    keyStore.setEcdhProvider(vault, provider);
    const offCurve = new Point(new Fr(1n), new Fr(1n));
    const [result] = await keyStore.addressSecretMul(vault, [offCurve]);
    expect(result?.isOnCurve()).toBe(true);
    expect(provider.calls).toHaveLength(0);
    await expect(keyStore.addressSecretMul(vault, [new Point(Fr.ZERO, Fr.ZERO)])).rejects.toThrow(
      /zero public key/,
    );
  });

  it("fails without a viewing provider", async () => {
    const point = await derivePublicKeyFromSecretKey(GrumpkinScalar.random());
    await expect(keyStore.addressSecretMul(vault, [point])).rejects.toThrow(/No viewing provider/);
  });
});

describe("ThresholdEcdhProvider", () => {
  it("combines t_v valid partials, names a corrupt member, and refuses without enough", async () => {
    const { ThresholdEcdhProvider } = await import("../src/index.ts");
    const { InProcessPartialsSource, dealTestCommittee } = await import("../src/testing/index.ts");
    const { addressContext } = await import("@ome-labs/wasm");
    const { toApiPoint } = await import("../src/index.ts");

    const committee = dealTestCommittee(2, 3);
    const preaddress = Fr.random().toString();
    const vault = AztecAddress.fromStringUnsafe(
      addressContext(preaddress, toApiPoint(committee.groupKey)).address,
    );
    const manifest = {
      protocolVersion: 0,
      vault: vault.toString(),
      epoch: 0,
      threshold: 2,
      viewingThreshold: 2,
      preaddress,
      groupKey: toApiPoint(committee.groupKey),
      members: committee.verificationShares.map((share, i) => ({
        index: i + 1,
        signerKey: toApiPoint(committee.groupKey),
        verificationShare: toApiPoint(share),
        identity: Fr.random().toString(),
        transport: Fr.random().toString(),
        addresses: [],
      })),
    };
    const point = await derivePublicKeyFromSecretKey(GrumpkinScalar.random());

    const honest = new ThresholdEcdhProvider(
      vault,
      manifest,
      new InProcessPartialsSource(committee, preaddress, [1, 2]),
    );
    const other = new ThresholdEcdhProvider(
      vault,
      manifest,
      new InProcessPartialsSource(committee, preaddress, [2, 3]),
    );
    const [expected] = await honest.mul([point], "decryption");
    expect(await other.mul([point], "decryption")).toEqual([expected]);

    const invalid: number[][] = [];
    const withCorrupt = new ThresholdEcdhProvider(
      vault,
      manifest,
      new InProcessPartialsSource(committee, preaddress, [1, 2, 3], { corrupt: [1] }),
      (members) => invalid.push(members),
    );
    expect(await withCorrupt.mul([point], "tagging")).toEqual([expected]);
    expect(invalid).toEqual([[1]]);

    const short = new ThresholdEcdhProvider(
      vault,
      manifest,
      new InProcessPartialsSource(committee, preaddress, [1, 2], { corrupt: [1] }),
    );
    await expect(short.mul([point], "decryption")).rejects.toThrow();
  });
});
