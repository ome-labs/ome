import { randomBytes } from "node:crypto";
import { mkdir, readFile, readdir, rename, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";

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

/** Every JSON file in `dir`, parsed. */
export async function readAllJson<T>(dir: string): Promise<T[]> {
  let names: string[];
  try {
    names = await readdir(dir);
  } catch {
    return [];
  }
  return Promise.all(
    names
      .filter((name) => name.endsWith(".json"))
      .map(async (name) => JSON.parse(await readFile(join(dir, name), "utf8")) as T),
  );
}
