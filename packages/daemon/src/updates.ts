// Whether a newer release of Ome exists: the daemon reads GitHub's release feed on start and once a day (nothing is
// sent but the request, and `config.set { updateCheck: false }` turns it off), and `status` names the newer release
// so `ome status` and `ome doctor` can say how to update. A failed check is silent: it is a convenience.
const FEED = "https://api.github.com/repos/ome-labs/ome/releases/latest";
const DAY_MS = 24 * 60 * 60 * 1000;

/** `major.minor.patch` as numbers, a leading `v` and a pre-release suffix ignored. */
const numbers = (version: string): number[] =>
  version
    .replace(/^v/u, "")
    .split("-")[0]
    ?.split(".")
    .map((part) => Number.parseInt(part, 10) || 0) ?? [];

/** Positive when release `a` is newer than `b`, negative when older, 0 when the same. */
export function compareReleases(a: string, b: string): number {
  const [x, y] = [numbers(a), numbers(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const difference = (x[i] ?? 0) - (y[i] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

export class UpdateCheck {
  readonly #current: string;
  readonly #enabled: () => boolean;
  readonly #fetch: typeof fetch;
  #newer: string | null = null;
  #timer: NodeJS.Timeout | undefined;

  /** Checks for releases newer than `current` while `enabled()`. */
  constructor(current: string, enabled: () => boolean, fetcher: typeof fetch = fetch) {
    this.#current = current;
    this.#enabled = enabled;
    this.#fetch = fetcher;
  }

  /** The newest release, when it is newer than this daemon. */
  get newer(): string | null {
    return this.#enabled() ? this.#newer : null;
  }

  start(): void {
    void this.check();
    this.#timer = setInterval(() => void this.check(), DAY_MS);
    this.#timer.unref();
  }

  stop(): void {
    clearInterval(this.#timer);
  }

  /** Reads the feed once; keeps the last answer when it can't. */
  async check(): Promise<void> {
    if (!this.#enabled()) return;
    try {
      const response = await this.#fetch(FEED, {
        headers: { accept: "application/vnd.github+json" },
        signal: AbortSignal.timeout(5000),
      });
      if (!response.ok) return;
      const { tag_name: tag } = (await response.json()) as { tag_name?: unknown };
      if (typeof tag !== "string") return;
      const latest = tag.replace(/^v/u, "");
      this.#newer = compareReleases(latest, this.#current) > 0 ? latest : null;
    } catch {
      // Offline, rate-limited or blocked: no notice.
    }
  }
}
