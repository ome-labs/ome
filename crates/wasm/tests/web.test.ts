// The browser build loads through its ES-module glue (`initSync` with the module bytes, as a bundler or `init()` would)
// and reaches `crypto.getRandomValues` for randomness. Node 24 provides both, so no browser is needed here.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import * as web from "../dist/main/web/ome_wasm.js";

describe("browser build", () => {
  it("initializes from its bytes and generates a signer", () => {
    web.initSync({
      module: readFileSync(join(import.meta.dirname, "../dist/main/web/ome_wasm_bg.wasm")),
    });
    const key = `0x00${"11".repeat(31)}`;
    const sealed = web.generateSigner({ key });
    const signer = web.UnlockedSigner.open(sealed, { key });
    expect(signer.publicKey()).toEqual(web.sealedPublicKey(sealed));
    signer.free();
    expect(web.selector("transfer_private_to_private((Field),(Field),u128,Field)")).toBe(
      0xedc09d49,
    );
  });
});
