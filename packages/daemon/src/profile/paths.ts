import { createHash } from "node:crypto";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

import { DaemonError } from "../errors.ts";

/** Profile names: lowercase letters, digits, `-` and `_`, at most 32, so a name cannot leave the home. */
const PROFILE_NAME = /^[a-z0-9][a-z0-9_-]{0,31}$/u;

/** Longest socket path every supported OS accepts (macOS allows 104 bytes). */
const MAX_SOCKET_PATH = 100;

/** Where one profile keeps everything. The layout `ome-client` discovers: `<home>/profiles/<name>/daemon.{addr,token}`. */
export interface ProfilePaths {
  readonly name: string;
  readonly dir: string;
  /** Public profile, settings, contacts, aliases, tokens. */
  readonly state: string;
  /** The sealed signing key. */
  readonly signer: string;
  /** The keeper's master key, when no OS keychain holds it, sealed with the signer's sealing key. */
  readonly masterKey: string;
  /** Secrets sealed with the master key: the member's account, vault protocol secrets. */
  readonly secrets: string;
  readonly vaults: string;
  readonly rotations: string;
  readonly keeperDir: string;
  readonly keeperSocket: string;
  readonly keeperLog: string;
  readonly pxe: string;
  readonly socket: string;
  readonly addr: string;
  readonly token: string;
  readonly lock: string;
}

/** `OME_HOME`, or `~/.ome`. */
export function omeHome(): string {
  return process.env.OME_HOME || join(homedir(), ".ome");
}

/** A socket path under `dir`, or a short one in the temporary directory when that would be too long. */
function socketPath(dir: string, file: string): string {
  const path = join(dir, file);
  if (Buffer.byteLength(path) <= MAX_SOCKET_PATH) return path;
  const tag = createHash("sha256").update(path).digest("hex").slice(0, 16);
  return join(tmpdir(), `ome-${tag}-${file}`);
}

export function profilePaths(name: string, home = omeHome()): ProfilePaths {
  if (!PROFILE_NAME.test(name))
    throw new DaemonError("invalid", `${JSON.stringify(name)} is not a profile name`);
  const dir = join(home, "profiles", name);
  const keeperDir = join(dir, "keeper");
  return {
    name,
    dir,
    state: join(dir, "profile.json"),
    signer: join(dir, "signer.json"),
    masterKey: join(dir, "keeper-key.sealed"),
    secrets: join(dir, "secrets"),
    vaults: join(dir, "vaults"),
    rotations: join(dir, "rotations"),
    keeperDir,
    keeperSocket: socketPath(keeperDir, "keeper.sock"),
    keeperLog: join(dir, "keeper.log"),
    pxe: join(dir, "pxe"),
    socket: socketPath(dir, "daemon.sock"),
    addr: join(dir, "daemon.addr"),
    token: join(dir, "daemon.token"),
    lock: join(dir, "daemon.lock"),
  };
}
