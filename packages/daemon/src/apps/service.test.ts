// Which apps may use a vault: connections are off until the member turns them on, an app is answered only once the
// member allowed its origin (asking is announced), origins are web origins only, and revoking ends it.
import type { DaemonEvent } from "@ome-labs/api";
import { describe, expect, it } from "vitest";

import type { Context } from "../context.ts";
import type { ProfileFile } from "../profile/state.ts";
import type { ProposalService } from "../proposals/service.ts";
import { AppService } from "./service.ts";

const VAULT = `0x${"0a".repeat(32)}` as const;

function service() {
  const file = { settings: { appsEnabled: false }, apps: {} } as unknown as ProfileFile;
  const events: DaemonEvent[] = [];
  const context = {
    state: {
      file,
      update: async (change: (f: ProfileFile) => void) => change(file),
    },
    vaults: {
      record: (vault: string) => {
        if (vault !== VAULT) throw new Error("unknown vault");
        return { name: "Treasury" };
      },
    },
    emit: (event: DaemonEvent) => events.push(event),
  } as unknown as Context;
  return { apps: new AppService(context, {} as ProposalService), file, events };
}

describe("apps", () => {
  it("answers an allowed app only while connections are on, and announces one that asks first", async () => {
    const { apps, file, events } = service();
    expect(() => apps.app("https://swap.example")).toThrow(/connections are off/u);
    file.settings.appsEnabled = true;
    expect(() => apps.app("https://swap.example")).toThrow(/isn't allowed/u);
    expect(events).toEqual([{ type: "appConnect", origin: "https://swap.example" }]);

    const view = await apps.allow({ origin: "https://Swap.example", vault: VAULT, name: "Swap" });
    expect(view).toMatchObject({ origin: "https://swap.example", vault: VAULT, name: "Swap" });
    expect(apps.app("https://swap.example").vault).toBe(VAULT);
    expect(apps.list()).toHaveLength(1);

    await apps.revoke("https://swap.example");
    expect(apps.list()).toEqual([]);
    await expect(apps.revoke("https://swap.example")).rejects.toThrow(/isn't an allowed app/u);
  });

  it("takes web origins only, for a vault the member is in, and refuses a call it doesn't know", async () => {
    const { apps, file } = service();
    file.settings.appsEnabled = true;
    for (const origin of ["swap.example", "https://swap.example/path", "javascript:alert(1)", ""])
      // oxlint-disable-next-line no-await-in-loop
      await expect(apps.allow({ origin, vault: VAULT })).rejects.toThrow(/not a web origin/u);
    await expect(
      apps.allow({ origin: "http://localhost:5173", vault: `0x${"0b".repeat(32)}` }),
    ).rejects.toThrow(/unknown vault/u);
    await apps.allow({ origin: "http://localhost:5173", vault: VAULT });
    await expect(
      apps.call({ origin: "http://localhost:5173", method: "signMessage", args: "[]" }),
    ).rejects.toThrow(/isn't a wallet method/u);
    await expect(
      apps.call({ origin: "http://localhost:5173", method: "getAccounts", args: "{}" }),
    ).rejects.toThrow(/JSON list/u);
  });
});
