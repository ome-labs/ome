// The update check names a release newer than this daemon from GitHub's feed, says nothing when it's off, older or
// unreachable, and compares releases numerically.
import { describe, expect, it } from "vitest";

import { UpdateCheck, compareReleases } from "./updates.ts";

const feed = (tag: string) =>
  (() =>
    Promise.resolve(new Response(JSON.stringify({ tag_name: tag })))) as unknown as typeof fetch;

describe("update check", () => {
  it("names a newer release, and nothing otherwise", async () => {
    const newer = new UpdateCheck("0.2.0", () => true, feed("v0.10.1"));
    await newer.check();
    expect(newer.newer).toBe("0.10.1");

    const same = new UpdateCheck("0.2.0", () => true, feed("v0.2.0"));
    await same.check();
    expect(same.newer).toBeNull();

    let on = false;
    const off = new UpdateCheck("0.2.0", () => on, feed("v9.0.0"));
    await off.check();
    expect(off.newer).toBeNull();
    on = true;
    await off.check();
    expect(off.newer).toBe("9.0.0");

    const offline = new UpdateCheck("0.2.0", () => true, (() =>
      Promise.reject(new Error("offline"))) as unknown as typeof fetch);
    await offline.check();
    expect(offline.newer).toBeNull();
  });

  it("compares releases by number, ignoring a pre-release suffix", () => {
    expect(compareReleases("0.10.0", "0.9.9")).toBeGreaterThan(0);
    expect(compareReleases("v1.0.0", "1.0.0")).toBe(0);
    expect(compareReleases("1.0.0-rc.1", "1.0.0")).toBe(0);
    expect(compareReleases("0.2.0", "0.2.1")).toBeLessThan(0);
  });
});
