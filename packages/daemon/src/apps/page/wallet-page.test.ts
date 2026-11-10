// The app page speaks wallet-sdk's iframe protocol: this test plays the app with wallet-sdk's own crypto. Only an
// allowed origin is answered, the key exchange agrees on the verification hash both sides show, each wallet call
// reaches the daemon decrypted and its answer comes back encrypted, and nothing crosses to another origin's session.
import * as crypto from "@aztec-labs/wallet-sdk/crypto";
import { WalletMessageType } from "@aztec-labs/wallet-sdk/types";
import { describe, expect, it } from "vitest";

import { createWalletPage } from "./wallet-page.js";

const APP = "https://swap.example";
const OTHER = "https://evil.example";
const WALLET = { id: "ome", name: "Ome", version: "0.1.0" };

function page(allowedOrigins: string[]) {
  const posted: { message: Record<string, unknown>; origin: string }[] = [];
  const calls: { origin: string; method: string; args: string }[] = [];
  const handler = createWalletPage({
    crypto,
    types: WalletMessageType,
    wallet: WALLET,
    post: (message, origin) => posted.push({ message: message as Record<string, unknown>, origin }),
    allowed: (origin) => Promise.resolve(allowedOrigins.includes(origin)),
    call: (origin, method, args) => {
      calls.push({ origin, method, args });
      if (method === "fail") return Promise.reject(new Error("refused"));
      return Promise.resolve([{ alias: "Treasury", item: "0x0a" }]);
    },
  });
  return { handler, posted, calls };
}

/** Discovery and key exchange as wallet-sdk's app side does them; returns the session and its key. */
async function connect(
  handler: ReturnType<typeof page>["handler"],
  posted: ReturnType<typeof page>["posted"],
) {
  const requestId = "request-1";
  await handler.handle({ type: WalletMessageType.DISCOVERY, requestId, appId: "swap" }, APP);
  const keyPair = await crypto.generateKeyPair();
  await handler.handle(
    {
      type: WalletMessageType.KEY_EXCHANGE_REQUEST,
      requestId,
      publicKey: await crypto.exportPublicKey(keyPair.publicKey),
    },
    APP,
  );
  const response = posted.at(-1)?.message as { publicKey: unknown; verificationHash: string };
  const keys = await crypto.deriveSessionKeys(
    keyPair,
    await crypto.importPublicKey(response.publicKey as never),
    true,
  );
  return {
    sessionId: requestId,
    key: keys.encryptionKey,
    hashes: [keys.verificationHash, response.verificationHash],
  };
}

describe("the app page", () => {
  it("answers only an allowed origin's discovery", async () => {
    const { handler, posted } = page([APP]);
    await handler.handle(
      { type: WalletMessageType.DISCOVERY, requestId: "x", appId: "evil" },
      OTHER,
    );
    expect(posted).toEqual([]);
    await handler.handle({ type: WalletMessageType.DISCOVERY, requestId: "y", appId: "swap" }, APP);
    expect(posted).toEqual([
      {
        message: { type: WalletMessageType.DISCOVERY_RESPONSE, requestId: "y", walletInfo: WALLET },
        origin: APP,
      },
    ]);
  });

  it("agrees on the session, forwards each call decrypted, and returns the answer encrypted", async () => {
    const { handler, posted, calls } = page([APP]);
    const session = await connect(handler, posted);
    const [mine, theirs] = session.hashes;
    expect(mine).toBe(theirs);

    const ask = async (type: string, args: unknown[], origin = APP) => {
      const before = posted.length;
      await handler.handle(
        {
          type: WalletMessageType.SECURE_MESSAGE,
          sessionId: session.sessionId,
          encrypted: await crypto.encrypt(
            session.key,
            JSON.stringify({ messageId: "m1", type, args, appId: "swap" }),
          ),
        },
        origin,
      );
      const reply = posted.slice(before).at(0);
      if (!reply) return undefined;
      expect(reply.origin).toBe(APP);
      return crypto.decrypt(session.key, reply.message.encrypted as never);
    };

    expect(await ask("getAccounts", [])).toEqual({
      messageId: "m1",
      walletId: "ome",
      result: [{ alias: "Treasury", item: "0x0a" }],
    });
    expect(calls).toEqual([{ origin: APP, method: "getAccounts", args: "[]" }]);
    expect(await ask("fail", [1])).toMatchObject({ messageId: "m1", error: "refused" });
    // Another origin can't use this session.
    expect(await ask("getAccounts", [], OTHER)).toBeUndefined();
    expect(calls).toHaveLength(2);

    await handler.handle({ type: WalletMessageType.PING, sessionId: session.sessionId }, APP);
    expect(posted.at(-1)?.message).toEqual({
      type: WalletMessageType.PONG,
      sessionId: session.sessionId,
    });
    await handler.handle({ type: WalletMessageType.DISCONNECT, sessionId: session.sessionId }, APP);
    expect(await ask("getAccounts", [])).toBeUndefined();
  });

  it("refuses a key exchange no allowed discovery started", async () => {
    const { handler, posted } = page([APP]);
    const keyPair = await crypto.generateKeyPair();
    await handler.handle(
      {
        type: WalletMessageType.KEY_EXCHANGE_REQUEST,
        requestId: "never-discovered",
        publicKey: await crypto.exportPublicKey(keyPair.publicKey),
      },
      APP,
    );
    expect(posted).toEqual([]);
  });
});
