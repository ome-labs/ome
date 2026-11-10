// The app page server: it serves the page and wallet-sdk's modules, and answers its two calls only for the page
// itself (the `x-ome-page` header, and no other site's Origin), forwarding wallet calls to the app service.
import { afterEach, describe, expect, it } from "vitest";

import { AppPageServer } from "./page-server.ts";
import type { AppService } from "./service.ts";

const APP = "https://swap.example";

const apps = {
  app: (origin: string) => {
    if (origin !== APP) throw new Error("not allowed");
    return { vault: "0x0a", name: null, allowedAt: 0 };
  },
  call: (params: { origin: string; method: string; args: string }) =>
    params.method === "getAccounts"
      ? Promise.resolve({ result: JSON.stringify([{ alias: "Treasury", item: params.args }]) })
      : Promise.reject(new Error(`${params.method} isn't a wallet method`)),
} as unknown as AppService;

let server: AppPageServer | undefined;
afterEach(async () => {
  await server?.close();
});

describe("the app page server", () => {
  it("serves the page and wallet-sdk's modules", async () => {
    server = await AppPageServer.listen(0, apps);
    const page = await fetch(`${server.origin}/`);
    expect(page.headers.get("content-type")).toMatch(/text\/html/u);
    expect(await page.text()).toContain("main.js");
    const sdk = await fetch(`${server.origin}/sdk/crypto.js`);
    expect(sdk.headers.get("content-type")).toBe("text/javascript");
    expect(await sdk.text()).toContain("deriveSessionKeys");
    expect((await fetch(`${server.origin}/../package.json`)).status).toBe(404);
  });

  it("answers its calls only for the page, and forwards wallet calls", async () => {
    server = await AppPageServer.listen(0, apps);
    const allowed = (origin: string, headers: Record<string, string>) =>
      fetch(`${server?.origin}/api/allowed?origin=${encodeURIComponent(origin)}`, { headers });
    expect((await allowed(APP, {})).status).toBe(403);
    expect((await allowed(APP, { "x-ome-page": "1", origin: "https://evil.example" })).status).toBe(
      403,
    );
    expect(await (await allowed(APP, { "x-ome-page": "1" })).json()).toEqual({ allowed: true });
    expect(await (await allowed("https://evil.example", { "x-ome-page": "1" })).json()).toEqual({
      allowed: false,
    });

    const call = async (method: string) =>
      (
        await fetch(`${server?.origin}/api/call`, {
          method: "POST",
          headers: { "x-ome-page": "1", origin: server?.origin ?? "" },
          body: JSON.stringify({ origin: APP, method, args: "[]" }),
        })
      ).json();
    expect(await call("getAccounts")).toEqual({ result: [{ alias: "Treasury", item: "[]" }] });
    expect(await call("signMessage")).toEqual({ error: "signMessage isn't a wallet method" });
  });
});
