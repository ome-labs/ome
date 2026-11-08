import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { writePrivate } from "../profile/files.ts";
import { open, seal } from "./sealing.ts";

/**
 * The daemon's secrets at rest, each file sealed with the keeper's master key: the member's own Aztec account and each
 * vault's shared protocol secrets. Viewing shares are never here; they live only in the keeper.
 */
export class SecretsStore {
  readonly #dir: string;
  readonly #key: Buffer;

  constructor(dir: string, key: Buffer) {
    this.#dir = dir;
    this.#key = key;
  }

  async put(name: string, value: Uint8Array): Promise<void> {
    await writePrivate(join(this.#dir, `${name}.sealed`), seal(this.#key, value));
  }

  async get(name: string): Promise<Buffer | null> {
    try {
      return open(this.#key, await readFile(join(this.#dir, `${name}.sealed`)));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
      throw error;
    }
  }
}
