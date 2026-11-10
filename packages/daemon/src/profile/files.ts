import { randomBytes } from "node:crypto";
import { mkdir, readFile, readdir, rename, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";

import { type Schema, backup, current, upgrade } from "./schema.ts";

/** Creates `dir` (and parents) readable only by the owner. */
export async function privateDir(dir: string): Promise<void> {
  await mkdir(dir, { recursive: true, mode: 0o700 });
}

/** Writes `contents` owner-only and atomically (temporary file, then rename). */
export async function writePrivate(path: string, contents: string | Uint8Array): Promise<void> {
  await privateDir(dirname(path));
  const temporary = `${path}.${randomBytes(4).toString("hex")}.tmp`;
  await writeFile(temporary, contents, { mode: 0o600 });
  await rename(temporary, path);
}

/** `path` parsed as JSON, or `fallback` when it does not exist. */
export async function readJson<T>(path: string, fallback: T): Promise<T> {
  try {
    return JSON.parse(await readFile(path, "utf8")) as T;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return fallback;
    throw error;
  }
}

export const writeJson = (path: string, value: unknown): Promise<void> =>
  writePrivate(path, `${JSON.stringify(value, null, 2)}\n`);

/**
 * `path` at `schema`'s current version, or `fallback` when it does not exist. An older file is migrated and written
 * back (the old one kept as `<path>.bak-<schema>`); a newer one is refused.
 */
export async function readVersioned<T>(path: string, schema: Schema, fallback: T): Promise<T> {
  let raw: Record<string, unknown>;
  try {
    raw = JSON.parse(await readFile(path, "utf8")) as Record<string, unknown>;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return fallback;
    throw error;
  }
  const { value, from, migrated } = upgrade<T>(schema, raw, path);
  if (migrated) {
    await backup(path, from);
    await writeJson(path, value);
  }
  return value;
}

/** Writes `value` to `path` stamped with `schema`'s current version. */
export const writeVersioned = (path: string, schema: Schema, value: object): Promise<void> =>
  writeJson(path, { ...value, schema: current(schema) });

/** Every JSON file in `dir`, each at `schema`'s current version (see {@link readVersioned}). */
export async function readAllVersioned<T>(dir: string, schema: Schema): Promise<T[]> {
  let names: string[];
  try {
    names = await readdir(dir);
  } catch {
    return [];
  }
  const files = await Promise.all(
    names
      .filter((name) => name.endsWith(".json"))
      .map((name) => readVersioned<T | null>(join(dir, name), schema, null)),
  );
  return files.filter((file): file is NonNullable<typeof file> => file !== null) as T[];
}
