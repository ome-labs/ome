// Where each vector set is written, the header every file carries, and the comparison used by `--check`.

import { readFile, writeFile } from "node:fs/promises";
import { isDeepStrictEqual } from "node:util";

const repository = new URL("../../../", import.meta.url);

/** The Aztec version the workspace pins (`.aztecrc`). */
export const aztecVersion = (await readFile(new URL(".aztecrc", repository), "utf8")).trim();

const { name, version } = JSON.parse(
  await readFile(new URL("../package.json", import.meta.url), "utf8"),
);

/** Identifies what produced a file and against which Aztec release. */
export const header = { source: `${name} ${version}`, aztecVersion };

/** A vector file's location in the repository. */
export const target = (path) => new URL(path, repository);

/** The committed contents of a vector file, or `undefined` before its first generation. */
export async function readCommitted(url) {
  try {
    return JSON.parse(await readFile(url, "utf8"));
  } catch (error) {
    if (error.code === "ENOENT") return undefined;
    throw error;
  }
}

export const write = (url, vectors) => writeFile(url, `${JSON.stringify(vectors, null, 2)}\n`);

/** Compares data, not bytes: the repository formatter may reflow committed JSON. */
export const matches = (committed, vectors) => isDeepStrictEqual(committed, vectors);
