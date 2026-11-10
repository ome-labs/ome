// Pages of a list, newest first. Each item has a time and an id; the cursor is the last item's, and the next page
// holds the items strictly older than it, so items that arrive meanwhile never shift a page.
import { DaemonError, require } from "./errors.ts";

/** What orders an item: its time, then its id. */
export interface Key {
  readonly at: number;
  readonly id: string;
}

/** A page request: at most `limit` items (50 when absent), older than `before`. */
export interface PageRequest {
  readonly limit?: number | undefined;
  readonly before?: string | undefined;
}

export const DEFAULT_LIMIT = 50;
export const MAX_LIMIT = 500;

const newer = (a: Key, b: Key) => a.at > b.at || (a.at === b.at && a.id > b.id);

function encode(key: Key): string {
  return Buffer.from(JSON.stringify([key.at, key.id])).toString("base64url");
}

function decode(cursor: string): Key {
  try {
    const [at, id] = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as unknown[];
    if (typeof at === "number" && typeof id === "string") return { at, id };
  } catch {
    // Reported below.
  }
  throw new DaemonError("invalid", "this cursor isn't one the daemon gave");
}

/** The page of `items` `request` asks for, newest first, and the cursor of the next one. */
export function page<T>(
  items: readonly T[],
  key: (item: T) => Key,
  request: PageRequest,
): { items: T[]; next: string | null } {
  const limit = request.limit ?? DEFAULT_LIMIT;
  require(Number.isInteger(limit) &&
    limit >= 1 &&
    limit <= MAX_LIMIT, `the limit is 1 to ${MAX_LIMIT}`);
  const before = request.before === undefined ? undefined : decode(request.before);
  const sorted = items
    .map((item) => ({ item, key: key(item) }))
    .filter(({ key: k }) => !before || newer(before, k))
    .toSorted((a, b) => (newer(a.key, b.key) ? -1 : newer(b.key, a.key) ? 1 : 0));
  const taken = sorted.slice(0, limit);
  const last = taken.at(-1);
  return {
    items: taken.map(({ item }) => item),
    next: sorted.length > limit && last ? encode(last.key) : null,
  };
}
