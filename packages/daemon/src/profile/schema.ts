// Versioned profile files. Each JSON file the daemon keeps carries `schema`, and a list of migrations takes a file
// from one schema to the next: a release that changes a file's shape appends one, and a member's files upgrade
// themselves on the next start, the previous file kept beside it (`<file>.bak-<schema>`). A file written by a newer
// release is refused, never read with this release's guesses. A file without `schema` was written before files had
// one, which is schema 0.
import { copyFile } from "node:fs/promises";

/** One step: a file of the schema before it, as the schema after it. Pure; it may assume nothing newer. */
export type Migration = (file: Record<string, unknown>) => Record<string, unknown>;

/** The migrations of one kind of file, oldest first; its current schema is their count. */
export interface Schema {
  /** What the file is, for messages ("profile", "vault record"). */
  readonly name: string;
  readonly migrations: readonly Migration[];
}

/** A file's schema: `schema`, or 0 when it has none. */
export const schemaOf = (file: Record<string, unknown>): number =>
  typeof file.schema === "number" ? file.schema : 0;

/** The current schema of `schema`'s files. */
export const current = (schema: Schema): number => schema.migrations.length;

/** Thrown for a file a newer release wrote. */
export class NewerFileError extends Error {
  constructor(name: string, path: string, found: number, known: number) {
    super(
      `${path} is a ${name} written by a newer Ome (schema ${found}; this release knows ${known}): update Ome`,
    );
    this.name = "NewerFileError";
  }
}

/**
 * `file` at the current schema. Refuses a newer one; migrates an older one step by step and says so, so the caller
 * writes it back (after {@link backup}).
 */
export function upgrade<T>(
  schema: Schema,
  file: Record<string, unknown>,
  path: string,
): { value: T; from: number; migrated: boolean } {
  const from = schemaOf(file);
  const known = current(schema);
  if (from > known) throw new NewerFileError(schema.name, path, from, known);
  let value = file;
  for (const migration of schema.migrations.slice(from)) value = migration(value);
  return { value: { ...value, schema: known } as T, from, migrated: from < known };
}

/** Keeps `path` as it was at schema `from` before it is rewritten. */
export async function backup(path: string, from: number): Promise<void> {
  await copyFile(path, `${path}.bak-${from}`).catch((error: NodeJS.ErrnoException) => {
    if (error.code !== "ENOENT") throw error;
  });
}
