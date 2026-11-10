// Pages are newest first, and a cursor keeps its place while newer items arrive.
import { describe, expect, it } from "vitest";

import { page } from "./paging.ts";

interface Item {
  readonly at: number;
  readonly id: string;
}

const items = (n: number, from = 0): Item[] =>
  Array.from({ length: n }, (_, i) => ({ at: 1000 + from + i, id: `p${from + i}` }));

describe("page", () => {
  it("pages newest first and ends with no cursor", () => {
    const all = items(5);
    const first = page(all, (i) => i, { limit: 2 });
    expect(first.items.map((i) => i.id)).toEqual(["p4", "p3"]);
    const second = page(all, (i) => i, { limit: 2, before: first.next ?? undefined });
    expect(second.items.map((i) => i.id)).toEqual(["p2", "p1"]);
    const third = page(all, (i) => i, { limit: 2, before: second.next ?? undefined });
    expect(third).toEqual({ items: [{ at: 1000, id: "p0" }], next: null });
  });

  it("keeps its place when newer items arrive, and orders ties by id", () => {
    const first = page(items(4), (i) => i, { limit: 2 });
    const grown = [...items(4), ...items(3, 4), { at: 1001, id: "p1b" }];
    const second = page(grown, (i) => i, { limit: 3, before: first.next ?? undefined });
    expect(second.items.map((i) => i.id)).toEqual(["p1b", "p1", "p0"]);
  });

  it("refuses a limit out of range and a cursor it didn't give", () => {
    expect(() => page(items(1), (i) => i, { limit: 0 })).toThrow(/limit/u);
    expect(() => page(items(1), (i) => i, { before: "nope" })).toThrow(/cursor/u);
  });
});
