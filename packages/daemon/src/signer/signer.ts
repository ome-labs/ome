import { readFile, rename } from "node:fs/promises";

import type { Hex32, Point, Secret, SignerStatus } from "@ome-labs/api";
import {
  UnlockedSigner,
  generateSigner,
  resealSigner,
  sealedPublicKey,
  sealingKey,
} from "@ome-labs/wasm";

import { DaemonError, notReady } from "../errors.ts";
import { writePrivate } from "../profile/files.ts";

/**
 * The member's approval key: only its sealed JSON document is stored, and an unlocked key lives in WebAssembly memory
 * (`UnlockedSigner`) for an unlock window, then is wiped. Every signature goes through `ome-signer`'s checks.
 */
export class SignerService {
  readonly #path: string;
  #sealed: string | null = null;
  #unlocked: UnlockedSigner | null = null;
  #until: number | null = null;
  #timer: NodeJS.Timeout | undefined;

  constructor(path: string) {
    this.#path = path;
  }

  async load(): Promise<void> {
    // A passphrase change writes the new file beside the old one first: finish one that was interrupted.
    await rename(`${this.#path}.next`, this.#path).catch(() => undefined);
    try {
      this.#sealed = await readFile(this.#path, "utf8");
    } catch {
      this.#sealed = null;
    }
  }

  get exists(): boolean {
    return this.#sealed !== null;
  }

  /** Creates and seals a new key under `passphrase`; returns its sealing key. */
  async create(passphrase: string): Promise<Hex32> {
    if (this.#sealed) throw new DaemonError("invalid", "this profile already has a signer");
    const sealed = generateSigner({ passphrase });
    await writePrivate(this.#path, sealed);
    this.#sealed = sealed;
    return sealingKey(sealed, passphrase);
  }

  publicKey(): Point {
    return sealedPublicKey(this.#sealedOrThrow());
  }

  status(): SignerStatus {
    if (this.#until !== null && Date.now() / 1000 >= this.#until) this.lock();
    return { locked: this.#unlocked === null, unlockedUntil: this.#until };
  }

  /** Opens the key for `ttlSecs`; returns the sealing key, which also opens the keeper's sealed master key. */
  unlock(secret: Secret, ttlSecs: number): Hex32 {
    const signer = UnlockedSigner.open(this.#sealedOrThrow(), secret);
    this.lock();
    this.#unlocked = signer;
    this.#until = Math.floor(Date.now() / 1000) + ttlSecs;
    this.#timer = setTimeout(() => this.lock(), ttlSecs * 1000);
    this.#timer.unref();
    return "key" in secret ? secret.key : sealingKey(this.#sealedOrThrow(), secret.passphrase);
  }

  lock(): void {
    clearTimeout(this.#timer);
    this.#unlocked?.free();
    this.#unlocked = null;
    this.#until = null;
  }

  /** The sealed key under `next`, opened with `current`; nothing is written. */
  resealed(current: Secret, next: string): { sealed: string; key: Hex32 } {
    const sealed = resealSigner(this.#sealedOrThrow(), current, next);
    return { sealed, key: sealingKey(sealed, next) };
  }

  /** Takes a resealed key the caller has written in place: the old passphrase stops working from here on. */
  replaced(sealed: string): void {
    this.lock();
    this.#sealed = sealed;
  }

  get path(): string {
    return this.#path;
  }

  sealingKey(passphrase: string): Hex32 {
    return sealingKey(this.#sealedOrThrow(), passphrase);
  }

  /**
   * Runs `sign` with the unlocked key, or with a key opened from `secret` for this call only. Throws `locked` when
   * neither is available.
   */
  with<T>(secret: Secret | undefined, sign: (signer: UnlockedSigner) => T): T {
    this.status();
    if (this.#unlocked) return sign(this.#unlocked);
    if (!secret)
      throw new DaemonError("locked", "the signer is locked; unlock it or pass a secret");
    const once = UnlockedSigner.open(this.#sealedOrThrow(), secret);
    try {
      return sign(once);
    } finally {
      once.free();
    }
  }

  #sealedOrThrow(): string {
    if (!this.#sealed) throw notReady("profile", "create a profile first");
    return this.#sealed;
  }
}
