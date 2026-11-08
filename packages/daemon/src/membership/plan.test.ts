// A membership plan is checked before it is shared: the contract's removal limit, the thresholds, the members named,
// and the join requests, with dealers taken only from members who stay.
import { describe, expect, it } from "vitest";

import { fakeContext } from "../../test/fake-context.ts";
import { MembershipService } from "./service.ts";

describe("membership.plan", () => {
  it("plans a removal with staying dealers and refuses what the rules forbid", async () => {
    const fake = await fakeContext(3, 2);
    const service = new MembershipService(fake.context);
    const vault = fake.deployment.address.toString();
    const base = { vault, remove: [3], add: [], threshold: 2, viewingThreshold: 2 };

    const plan = await service.plan(base);
    expect(plan.failure).toBeNull();
    expect(plan.withinRemovalLimit).toBe(true);
    expect(plan.dealers).toEqual([1, 2]);
    expect(plan.next.members).toHaveLength(2);
    expect(plan.share).toMatch(/^ome-rotation-v0:/u);

    // With t_v = 2 only one member may leave, and two must stay to deal.
    expect((await service.plan({ ...base, remove: [2, 3] })).failure).toMatch(
      /must stay|can leave/u,
    );
    expect((await service.plan({ ...base, threshold: 3 })).failure).toMatch(/spending threshold/u);
    expect((await service.plan({ ...base, viewingThreshold: 1 })).failure).toMatch(
      /viewing threshold/u,
    );
    await expect(service.plan({ ...base, remove: [9] })).rejects.toMatchObject({ kind: "invalid" });
    await expect(service.plan({ ...base, add: ["ome-join-v0:AAAA"] })).rejects.toMatchObject({
      kind: "invalid",
    });
  });

  it("refuses to begin from a plan this member takes no part in", async () => {
    const fake = await fakeContext(3, 2);
    const service = new MembershipService(fake.context);
    const plan = await service.plan({
      vault: fake.deployment.address.toString(),
      remove: [1],
      add: [],
      threshold: 2,
      viewingThreshold: 2,
    });
    // This daemon's keeper is member 1, who leaves.
    await expect(service.begin(plan.share)).rejects.toMatchObject({ kind: "invalid" });
    await expect(service.begin("ome-rotation-v0:not-base64-json")).rejects.toMatchObject({
      kind: "invalid",
    });
  });
});
