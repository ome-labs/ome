import { execFile } from "node:child_process";
import { randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import { promisify } from "node:util";

import { notReady } from "../errors.ts";
import { writePrivate } from "../profile/files.ts";
import { open, seal } from "./sealing.ts";

const run = promisify(execFile);
const SERVICE = "ome-keeper-master-key";

/**
 * Where the keeper's 32-byte master key lives. The OS keychain holds it where one exists (macOS Keychain, Linux
 * Secret Service through `secret-tool`), so the keeper can start without the member. Elsewhere, or with
 * `OME_KEYCHAIN=off`, it is a file sealed with the signer's sealing key: the keeper then starts at the first unlock.
 */
export interface MasterKeyStore {
  readonly kind: "keychain" | "file";
  /** The key, or `null` while it cannot be read without the member (a sealed file before the first unlock). */
  read(sealingKey?: Uint8Array): Promise<Buffer | null>;
  /** Creates and stores a new key. */
  create(sealingKey: Uint8Array): Promise<Buffer>;
  /**
   * The sealed file's new content under `next`, opened with `current`; `null` when a keychain holds the key (the
   * passphrase doesn't protect it). The caller writes it, so a passphrase change replaces both files together.
   */
  resealed(current: Uint8Array, next: Uint8Array): Promise<Buffer | null>;
  /** Deletes the key: from the keychain, or (for a sealed file) nothing, since the profile directory goes with it. */
  destroy(): Promise<void>;
}

class MacKeychain implements MasterKeyStore {
  readonly kind = "keychain";
  readonly #account: string;

  constructor(account: string) {
    this.#account = account;
  }

  async read(): Promise<Buffer | null> {
    try {
      const { stdout } = await run("security", [
        "find-generic-password",
        "-a",
        this.#account,
        "-s",
        SERVICE,
        "-w",
      ]);
      return Buffer.from(stdout.trim(), "hex");
    } catch {
      return null;
    }
  }

  async create(): Promise<Buffer> {
    const key = randomBytes(32);
    await run("security", [
      "add-generic-password",
      "-U",
      "-a",
      this.#account,
      "-s",
      SERVICE,
      "-w",
      key.toString("hex"),
    ]);
    return key;
  }

  resealed(): Promise<Buffer | null> {
    return Promise.resolve(null);
  }

  async destroy(): Promise<void> {
    await run("security", ["delete-generic-password", "-a", this.#account, "-s", SERVICE]).catch(
      () => undefined,
    );
  }
}

class SecretService implements MasterKeyStore {
  readonly kind = "keychain";
  readonly #account: string;

  constructor(account: string) {
    this.#account = account;
  }

  async read(): Promise<Buffer | null> {
    try {
      const { stdout } = await run("secret-tool", [
        "lookup",
        "service",
        SERVICE,
        "account",
        this.#account,
      ]);
      return stdout.trim() ? Buffer.from(stdout.trim(), "hex") : null;
    } catch {
      return null;
    }
  }

  async create(): Promise<Buffer> {
    const key = randomBytes(32);
    const child = execFile("secret-tool", [
      "store",
      "--label",
      "Ome keeper",
      "service",
      SERVICE,
      "account",
      this.#account,
    ]);
    child.stdin?.end(key.toString("hex"));
    await new Promise<void>((resolve, reject) => {
      child.once("exit", (code) =>
        code === 0 ? resolve() : reject(new Error(`secret-tool exited with ${code}`)),
      );
      child.once("error", reject);
    });
    return key;
  }

  resealed(): Promise<Buffer | null> {
    return Promise.resolve(null);
  }

  async destroy(): Promise<void> {
    await run("secret-tool", ["clear", "service", SERVICE, "account", this.#account]).catch(
      () => undefined,
    );
  }
}

class SealedFile implements MasterKeyStore {
  readonly kind = "file";
  readonly #path: string;

  constructor(path: string) {
    this.#path = path;
  }

  async read(sealingKey?: Uint8Array): Promise<Buffer | null> {
    if (!sealingKey) return null;
    try {
      return open(sealingKey, await readFile(this.#path));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
      throw error;
    }
  }

  async create(sealingKey: Uint8Array): Promise<Buffer> {
    const key = randomBytes(32);
    await writePrivate(this.#path, seal(sealingKey, key));
    return key;
  }

  async resealed(current: Uint8Array, next: Uint8Array): Promise<Buffer | null> {
    const key = await this.read(current);
    // No file yet: the keeper has never started, and creates it under the new key when it does.
    return key ? seal(next, key) : null;
  }

  destroy(): Promise<void> {
    return Promise.resolve();
  }
}

async function has(command: string): Promise<boolean> {
  try {
    await run("which", [command]);
    return true;
  } catch {
    return false;
  }
}

/** The best store available for profile `account`, keeping the sealed file at `path` as the fallback. */
export async function masterKeyStore(account: string, path: string): Promise<MasterKeyStore> {
  if (process.env.OME_KEYCHAIN !== "off") {
    if (process.platform === "darwin" && (await has("security")))
      return new MacKeychain(`ome:${account}`);
    if (process.platform === "linux" && (await has("secret-tool")))
      return new SecretService(`ome:${account}`);
  }
  return new SealedFile(path);
}

/** Throws `not-ready` while the keeper's key cannot be read yet. */
export function requireKey(key: Buffer | null): Buffer {
  if (!key) throw notReady("signer", "unlock the signer to start the keeper");
  return key;
}
