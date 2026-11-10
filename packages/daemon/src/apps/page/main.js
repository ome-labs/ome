// Boots the wallet page inside an app's iframe: wallet-sdk's modules from this server, calls to the daemon behind it.
import * as crypto from "./sdk/crypto.js";
import { WalletMessageType } from "./sdk/types.js";
import { createWalletPage } from "./wallet-page.js";

// A request without this header is refused: a website can't send it to this server without a preflight it never gets.
const HEADERS = { "content-type": "application/json", "x-ome-page": "1" };

const page = createWalletPage({
  crypto,
  types: WalletMessageType,
  wallet: { id: "ome", name: "Ome", version: "0.1.0" },
  post: (message, origin) => window.parent.postMessage(message, origin),
  allowed: async (origin) => {
    const answer = await fetch(`/api/allowed?origin=${encodeURIComponent(origin)}`, {
      headers: HEADERS,
    });
    return answer.ok && (await answer.json()).allowed === true;
  },
  call: async (origin, method, args) => {
    const answer = await fetch("/api/call", {
      method: "POST",
      headers: HEADERS,
      body: JSON.stringify({ origin, method, args }),
    });
    const { result, error } = await answer.json();
    if (error) throw new Error(error);
    return result;
  },
});

window.addEventListener("message", (event) => void page.handle(event.data, event.origin));
if (window.parent !== window)
  window.parent.postMessage({ type: WalletMessageType.WALLET_READY }, "*");
