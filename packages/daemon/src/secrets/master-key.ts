import { execFile } from "node:child_process";
import { randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import { promisify } from "node:util";

import { DaemonError } from "../errors.ts";
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
  if (!key) throw new DaemonError("not-ready", "unlock the signer to start the keeper");
  return key;
}
