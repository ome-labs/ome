// The wallet side of wallet-sdk's iframe protocol, for the page `omed` serves to apps (a development and test
// transport; the browser extension is the one members will use). It answers an allowed app's discovery, runs the key
// exchange, and forwards each decrypted wallet call to the daemon, which answers it as the vault's wallet.
//
// It mirrors wallet-sdk's `IframeConnectionHandler`, without its argument parsing: the daemon parses every call with
// aztec.js's `WalletSchema`. The crypto and message types are wallet-sdk's own modules, passed in so Node can test this.

/**
 * @typedef {object} PageDeps
 * @property {typeof import("@aztec-labs/wallet-sdk/crypto")} crypto wallet-sdk's crypto module.
 * @property {Record<string, string>} types wallet-sdk's `WalletMessageType`.
 * @property {(message: object, origin: string) => void} post Posts a message to the app at `origin`.
 * @property {(origin: string) => Promise<boolean>} allowed Whether the member allowed the app at `origin`.
 * @property {(origin: string, method: string, args: string) => Promise<unknown>} call One wallet call, through the
 *   daemon: arguments as JSON, the decoded result back.
 * @property {{ id: string, name: string, version: string, icon?: string }} wallet What the app shows for Ome.
 */

/** @param {PageDeps} deps */
export function createWalletPage({ crypto, types, post, allowed, call, wallet }) {
  /** @type {Map<string, { origin: string }>} discoveries the member allowed, by request id */
  const approved = new Map();
  /** @type {Map<string, { origin: string, key: CryptoKey }>} sessions, by id */
  const sessions = new Map();

  async function discovery({ requestId }, origin) {
    if (!(await allowed(origin))) return;
    approved.set(requestId, { origin });
    post({ type: types.DISCOVERY_RESPONSE, requestId, walletInfo: wallet }, origin);
  }

  async function keyExchange({ requestId, publicKey }, origin) {
    const pending = approved.get(requestId);
    if (!pending || pending.origin !== origin) return;
    approved.delete(requestId);
    const keyPair = await crypto.generateKeyPair();
    const keys = await crypto.deriveSessionKeys(
      keyPair,
      await crypto.importPublicKey(publicKey),
      false,
    );
    sessions.set(requestId, { origin, key: keys.encryptionKey });
    post(
      {
        type: types.KEY_EXCHANGE_RESPONSE,
        requestId,
        publicKey: await crypto.exportPublicKey(keyPair.publicKey),
        verificationHash: keys.verificationHash,
      },
      origin,
    );
  }

  async function secure({ sessionId, encrypted }, origin) {
    const session = sessions.get(sessionId);
    if (!session || session.origin !== origin) return;
    let request;
    try {
      request = await crypto.decrypt(session.key, encrypted);
    } catch {
      return;
    }
    const { messageId, type, args } = request;
    let result;
    let error;
    try {
      result = await call(origin, type, JSON.stringify(args ?? []));
    } catch (thrown) {
      error = thrown instanceof Error ? thrown.message : String(thrown);
    }
    const response = JSON.stringify({ messageId, walletId: wallet.id, result, error });
    post(
      {
        type: types.SECURE_RESPONSE,
        sessionId,
        encrypted: await crypto.encrypt(session.key, response),
      },
      origin,
    );
  }

  return {
    /** Handles one message from the app at `origin`. */
    async handle(message, origin) {
      if (!message || typeof message !== "object") return;
      switch (message.type) {
        case types.DISCOVERY:
          return discovery(message, origin);
        case types.KEY_EXCHANGE_REQUEST:
          return keyExchange(message, origin);
        case types.SECURE_MESSAGE:
          return secure(message, origin);
        case types.PING:
          if (sessions.get(message.sessionId)?.origin === origin)
            post({ type: types.PONG, sessionId: message.sessionId }, origin);
          return undefined;
        case types.DISCONNECT:
          if (sessions.get(message.sessionId)?.origin === origin)
            sessions.delete(message.sessionId);
          return undefined;
        default:
          return undefined;
      }
    },
  };
}
