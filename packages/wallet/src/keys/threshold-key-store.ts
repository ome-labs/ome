import type { Fr } from "@aztec-labs/foundation/curves/bn254";
import { type GrumpkinScalar, Point } from "@aztec-labs/foundation/curves/grumpkin";
import { KeyStore } from "@aztec-labs/key-store";
import type { AccountPrivacyKeys, AccountPrivacySecretKeys } from "@aztec-labs/key-store";
import type { AztecAsyncKVStore, AztecAsyncMap } from "@aztec-labs/kv-store";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import { CompleteAddress, type PartialAddress } from "@aztec-labs/stdlib/contract";
import { PublicKeys, derivePublicKeyFromSecretKey, hashPublicKey } from "@aztec-labs/stdlib/keys";

import type { EcdhProvider, EcdhPurpose } from "./ecdh.ts";

/**
 * Keys of an account whose incoming viewing secret is shared by a committee: the three privacy secrets every member
 * holds, the group incoming viewing key `Ivpk` from the DKG, and the public message-signing and fallback keys.
 */
export interface ThresholdAccountKeys {
  readonly kind: "threshold";
  readonly masterNullifierHidingSecretKey: GrumpkinScalar;
  readonly masterIncomingViewingPublicKey: Point;
  readonly masterOutgoingViewingSecretKey: GrumpkinScalar;
  readonly masterTaggingSecretKey: GrumpkinScalar;
  readonly masterMessageSigningPublicKey: Point;
  readonly masterFallbackPublicKey: Point;
}

/** Thrown when anything asks for a threshold account's incoming viewing secret. */
export class ThresholdViewingError extends Error {
  constructor(account: AztecAddress, operation: string) {
    super(
      `${operation}: ${account.toString()} is a threshold account; its incoming viewing secret is never held`,
    );
    this.name = "ThresholdViewingError";
  }
}

const isThresholdKeys = (
  keys: AccountPrivacyKeys | ThresholdAccountKeys,
): keys is ThresholdAccountKeys => "kind" in keys && keys.kind === "threshold";

const MARKER = "threshold";

/** Most remembered note-decryption and handshake results per store (oldest evicted). */
const DECRYPTION_CACHE_SIZE = 4096;

interface Pending {
  readonly points: Point[];
  readonly purpose: EcdhPurpose;
  readonly settle: { resolve: (point: Point) => void; reject: (error: unknown) => void }[];
}

/**
 * PXE key store that also holds threshold accounts (Ome vaults). A threshold account is stored like any other except
 * that its incoming viewing secret is absent: every `address_secret · P` the patched PXE needs goes through
 * {@link addressSecretMul} to the account's {@link EcdhProvider}, and any attempt to read the secret throws
 * {@link ThresholdViewingError}.
 *
 * Requests made in the same tick are batched into one provider call per account and purpose, and a point already in
 * flight is not requested twice. Results stay in memory for the session only, never on disk: tagging and sender-tag
 * results (counterparty address points) without bound, note-decryption and handshake results (one per message) in a
 * bounded LRU, because the PXE re-derives the same message secrets on every sync, simulation and proof. Both are
 * dropped when the account's provider changes (a new committee epoch).
 */
export class ThresholdKeyStore extends KeyStore {
  readonly #db: AztecAsyncKVStore;
  readonly #keys: AztecAsyncMap<string, Buffer>;
  readonly #threshold = new Set<string>();
  readonly #providers = new Map<string, EcdhProvider>();
  readonly #tagging = new Map<string, Point>();
  readonly #decryption = new Map<string, Point>();
  readonly #inflight = new Map<string, Promise<Point>>();
  readonly #pending = new Map<string, Pending>();

  constructor(database: AztecAsyncKVStore) {
    super(database);
    this.#db = database;
    // Same map as the base class: threshold accounts share its key layout, minus `ivsk_m`.
    this.#keys = database.openMap("key_store");
  }

  /** Registers an ordinary account, or a threshold account when given {@link ThresholdAccountKeys}. */
  override async addAccount(
    keys: AccountPrivacyKeys | ThresholdAccountKeys,
    partialAddress: PartialAddress,
  ): Promise<CompleteAddress> {
    return isThresholdKeys(keys)
      ? this.#addThresholdAccount(keys, partialAddress)
      : super.addAccount(keys, partialAddress);
  }

  override async getAccounts(): Promise<AztecAddress[]> {
    const threshold = await this.#thresholdAccounts();
    const ordinary = await super.getAccounts();
    return [...ordinary, ...threshold.filter((a) => !ordinary.some((o) => o.equals(a)))];
  }

  override async hasAccount(account: AztecAddress): Promise<boolean> {
    return (await this.isThresholdAccount(account)) || super.hasAccount(account);
  }

  override async getMasterIncomingViewingSecretKey(account: AztecAddress): Promise<GrumpkinScalar> {
    if (await this.isThresholdAccount(account)) {
      throw new ThresholdViewingError(account, "getMasterIncomingViewingSecretKey");
    }
    return super.getMasterIncomingViewingSecretKey(account);
  }

  override async getAccountSecretKeys(account: AztecAddress): Promise<AccountPrivacySecretKeys> {
    if (await this.isThresholdAccount(account)) {
      throw new ThresholdViewingError(account, "getAccountSecretKeys");
    }
    return super.getAccountSecretKeys(account);
  }

  override async getMasterSecretKey(pkMHash: Fr): Promise<GrumpkinScalar> {
    const [prefix, account] = await this.getKeyPrefixAndAccount(pkMHash);
    if (prefix === "iv" && (await this.isThresholdAccount(account))) {
      throw new ThresholdViewingError(account, "getMasterSecretKey");
    }
    return super.getMasterSecretKey(pkMHash);
  }

  /** Whether `account` is a threshold account (called by the patched PXE before every viewing-secret use). */
  async isThresholdAccount(account: AztecAddress): Promise<boolean> {
    const key = account.toString();
    if (this.#threshold.has(key)) return true;
    const marked =
      (await this.#db.transactionAsync(() => this.#keys.getAsync(`${key}-${MARKER}`))) !==
      undefined;
    if (marked) this.#threshold.add(key);
    return marked;
  }

  /** Routes `account`'s viewing operations to `provider` (held in memory; set after every start and epoch change). */
  setEcdhProvider(account: AztecAddress, provider: EcdhProvider): void {
    this.#providers.set(account.toString(), provider);
    const prefix = `${account.toString()}:`;
    for (const cache of [this.#tagging, this.#decryption]) {
      for (const key of cache.keys()) if (key.startsWith(prefix)) cache.delete(key);
    }
  }

  /**
   * `address_secret · P` for each point, for a threshold account (called by the patched PXE, which names the purpose
   * the keepers audit). Points that are not on the curve map to a random point, so a malformed ephemeral key in someone
   * else's log fails decryption instead of the whole sync; the identity is rejected as the stock PXE does.
   */
  async addressSecretMul(
    account: AztecAddress,
    points: Point[],
    options: { purpose?: EcdhPurpose } = {},
  ): Promise<Point[]> {
    const purpose = options.purpose ?? "decryption";
    return Promise.all(
      points.map((point) => {
        if (point.isZero())
          throw new Error("Attempting to derive a shared secret with a zero public key.");
        if (!point.isOnCurve()) return Point.random();
        return this.#lookup(account, point, purpose);
      }),
    );
  }

  #lookup(account: AztecAddress, point: Point, purpose: EcdhPurpose): Promise<Point> {
    const key = `${account.toString()}:${purpose}:${point.toString()}`;
    const lasting = purpose === "tagging" || purpose === "sender-tag";
    const cache = lasting ? this.#tagging : this.#decryption;
    const cached = cache.get(key);
    if (cached) {
      // Refresh LRU order.
      cache.delete(key);
      cache.set(key, cached);
      return Promise.resolve(cached);
    }
    const inflight = this.#inflight.get(key);
    if (inflight) return inflight;
    const request = this.#enqueue(account, point, purpose)
      .then((result) => {
        cache.set(key, result);
        if (cache === this.#decryption && cache.size > DECRYPTION_CACHE_SIZE) {
          cache.delete(cache.keys().next().value as string);
        }
        return result;
      })
      .finally(() => this.#inflight.delete(key));
    this.#inflight.set(key, request);
    return request;
  }

  #enqueue(account: AztecAddress, point: Point, purpose: EcdhPurpose): Promise<Point> {
    const key = `${account.toString()}:${purpose}`;
    let batch = this.#pending.get(key);
    if (!batch) {
      const created: Pending = { points: [], purpose, settle: [] };
      this.#pending.set(key, created);
      setImmediate(() => {
        this.#pending.delete(key);
        void this.#flush(account, created);
      });
      batch = created;
    }
    batch.points.push(point);
    const settle = batch.settle;
    return new Promise((resolve, reject) => {
      settle.push({ resolve, reject });
    });
  }

  async #flush(account: AztecAddress, batch: Pending): Promise<void> {
    try {
      const provider = this.#providers.get(account.toString());
      if (!provider)
        throw new Error(
          `No viewing provider registered for threshold account ${account.toString()}`,
        );
      const results = await provider.mul(batch.points, batch.purpose);
      if (results.length !== batch.points.length)
        throw new Error("Viewing provider returned a wrong-sized batch");
      batch.settle.forEach(({ resolve }, i) => resolve(results[i] as Point));
    } catch (error) {
      for (const { reject } of batch.settle) reject(error);
    }
  }

  async #thresholdAccounts(): Promise<AztecAddress[]> {
    const keys: string[] = [];
    await this.#db.transactionAsync(async () => {
      for await (const key of this.#keys.keysAsync())
        if (key.endsWith(`-${MARKER}`)) keys.push(key);
    });
    return keys.map((key) => AztecAddress.fromStringUnsafe(key.split("-")[0] as string));
  }

  async #addThresholdAccount(
    keys: ThresholdAccountKeys,
    partialAddress: PartialAddress,
  ): Promise<CompleteAddress> {
    const npk = await derivePublicKeyFromSecretKey(keys.masterNullifierHidingSecretKey);
    const ovpk = await derivePublicKeyFromSecretKey(keys.masterOutgoingViewingSecretKey);
    const tpk = await derivePublicKeyFromSecretKey(keys.masterTaggingSecretKey);
    const ivpk = keys.masterIncomingViewingPublicKey;
    for (const [name, point] of Object.entries({ npk, ivpk, ovpk, tpk })) {
      if (point.isInfinite || !point.isOnCurve())
        throw new Error(`Invalid ${name} for a threshold account`);
    }
    const publicKeys = new PublicKeys(
      await hashPublicKey(npk),
      ivpk,
      await hashPublicKey(ovpk),
      await hashPublicKey(tpk),
      await hashPublicKey(keys.masterMessageSigningPublicKey),
      await hashPublicKey(keys.masterFallbackPublicKey),
    );
    const completeAddress = await CompleteAddress.fromPublicKeysAndPartialAddress(
      publicKeys,
      partialAddress,
    );
    const account = completeAddress.address.toString();
    const entries: [string, Buffer][] = [
      // Same layout as `KeyStore`, without `ivsk_m`; the marker makes the account visible to `getAccounts`.
      ["ovsk_m", keys.masterOutgoingViewingSecretKey.toBuffer()],
      ["tsk_m", keys.masterTaggingSecretKey.toBuffer()],
      ["nhk_m", keys.masterNullifierHidingSecretKey.toBuffer()],
      ["npk_m", npk.toBuffer()],
      ["ivpk_m", ivpk.toBuffer()],
      ["ovpk_m", ovpk.toBuffer()],
      ["tpk_m", tpk.toBuffer()],
      ["npk_m_hash", publicKeys.npkMHash.toBuffer()],
      ["ivpk_m_hash", (await hashPublicKey(ivpk)).toBuffer()],
      ["ovpk_m_hash", publicKeys.ovpkMHash.toBuffer()],
      ["tpk_m_hash", publicKeys.tpkMHash.toBuffer()],
      [MARKER, Buffer.from([1])],
    ];
    await this.#db.transactionAsync(async () => {
      // Sequential writes inside one transaction, as the base class does.
      // oxlint-disable-next-line no-await-in-loop
      for (const [suffix, value] of entries) await this.#keys.set(`${account}-${suffix}`, value);
    });
    this.#threshold.add(account);
    return completeAddress;
  }
}
