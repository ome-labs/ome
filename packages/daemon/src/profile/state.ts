import type { Hex32, Profile, Settings, TokenInfo } from "@ome-labs/api";

import { readJson, writeJson } from "./files.ts";

/** Everything public a profile keeps: who the member is, settings, the address book, aliases and tracked tokens. */
export interface ProfileFile {
  profile: Profile | null;
  settings: Settings;
  aliases: Record<Hex32, string>;
  defaultVault: Hex32 | null;
  tokens: Record<Hex32, TokenInfo[]>;
}

export const DEFAULT_SETTINGS: Settings = {
  nodeUrl: "http://localhost:8080",
  relays: [],
  proverEnabled: false,
  feePayerSalt: `0x${"00".repeat(32)}`,
  unlockTimeoutSecs: 3600,
  remoteKeeper: null,
  aztecWalletDir: null,
  aztecWalletSync: false,
};

/** The profile file, read once and written on every change. */
export class ProfileState {
  readonly #path: string;
  #file: ProfileFile;

  private constructor(path: string, file: ProfileFile) {
    this.#path = path;
    this.#file = file;
  }

  static async load(path: string, settings: Partial<Settings> = {}): Promise<ProfileState> {
    const file = await readJson<ProfileFile>(path, {
      profile: null,
      settings: DEFAULT_SETTINGS,
      aliases: {},
      defaultVault: null,
      tokens: {},
    });
    file.settings = { ...DEFAULT_SETTINGS, ...file.settings, ...settings };
    return new ProfileState(path, file);
  }

  get file(): Readonly<ProfileFile> {
    return this.#file;
  }

  async update(change: (file: ProfileFile) => void): Promise<void> {
    change(this.#file);
    await writeJson(this.#path, this.#file);
  }
}
