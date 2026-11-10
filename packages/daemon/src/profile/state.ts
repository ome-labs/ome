import type { Hex32, HexBytes, Profile, Settings, TokenInfo } from "@ome-labs/api";

import { readVersioned, writeVersioned } from "./files.ts";
import type { Schema } from "./schema.ts";

/** A contract the member registered (its artifact is kept in the profile's `contracts` directory). */
export interface RegisteredContract {
  name: string;
  classId: Hex32;
  /** `SerializableContractInstance` bytes, for an instance the node doesn't publish. */
  instance: HexBytes | null;
}

/**
 * Everything public a profile keeps: who the member is, settings, the address book, aliases, tracked tokens and
 * registered contracts.
 */
export interface ProfileFile {
  profile: Profile | null;
  settings: Settings;
  aliases: Record<Hex32, string>;
  defaultVault: Hex32 | null;
  tokens: Record<Hex32, TokenInfo[]>;
  contracts: Record<Hex32, RegisteredContract>;
  /** Apps allowed to use a vault, by origin. */
  apps: Record<string, AppRecord>;
}

/** An app the member allowed (`apps.allow`). */
export interface AppRecord {
  vault: Hex32;
  name: string | null;
  allowedAt: number;
}

/** `profile.json`'s migrations. */
export const PROFILE_SCHEMA: Schema = {
  name: "profile",
  migrations: [
    // 1: the file gains `schema`; registered contracts and apps came after the first profiles.
    (file) => ({ contracts: {}, apps: {}, ...file }),
  ],
};

export const DEFAULT_SETTINGS: Settings = {
  nodeUrl: "http://localhost:8080",
  relays: [],
  proverEnabled: false,
  feePayerSalt: `0x${"00".repeat(32)}`,
  unlockTimeoutSecs: 3600,
  remoteKeeper: null,
  aztecWalletDir: null,
  aztecWalletSync: false,
  appsEnabled: false,
  updateCheck: true,
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
    const file = await readVersioned<ProfileFile>(path, PROFILE_SCHEMA, {
      profile: null,
      settings: DEFAULT_SETTINGS,
      aliases: {},
      defaultVault: null,
      tokens: {},
      contracts: {},
      apps: {},
    });
    // Settings added later default here: a setting is an open record, so a new one needs no migration.
    file.settings = { ...DEFAULT_SETTINGS, ...file.settings, ...settings };
    return new ProfileState(path, file);
  }

  get file(): Readonly<ProfileFile> {
    return this.#file;
  }

  async update(change: (file: ProfileFile) => void): Promise<void> {
    change(this.#file);
    await writeVersioned(this.#path, PROFILE_SCHEMA, this.#file);
  }
}
