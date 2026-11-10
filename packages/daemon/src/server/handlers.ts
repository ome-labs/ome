import type {
  ActivityEntry,
  DaemonMethod,
  DaemonParams,
  DaemonResult,
  Hex32,
  VaultSummary,
} from "@ome-labs/api";

import { listAliases, setAlias, walletDir } from "../aztec/aliases.ts";
import type { Daemon } from "../daemon.ts";
import { DaemonError, require } from "../errors.ts";
import { page } from "../paging.ts";
import { matches } from "../proposals/filters.ts";
import { counterparties, receivedTransfers } from "../vaults/activity.ts";
import { paymentRequest } from "../vaults/request.ts";
import { balances, deployTestToken, tokenInfo } from "../vaults/tokens.ts";

type Served = Exclude<DaemonMethod, "session.auth" | "events.subscribe">;
type Handlers = {
  [M in Served]: (daemon: Daemon, params: DaemonParams<M>) => Promise<DaemonResult<M>>;
};

const ALIAS = /^[a-z0-9][a-z0-9_-]{0,31}$/u;
/** The audit entries a page is cut from: the keeper's most recent. */
const AUDIT_WINDOW = 10_000;
const RELAY = /^\/(ip4|ip6|dns4|dns6|dns)\/.+\/p2p\/[1-9A-HJ-NP-Za-km-z]+$/u;
const HEX32 = /^0x[0-9a-f]{64}$/u;

const vaultList = (d: Daemon): VaultSummary[] => d.status().vaults;
const lower = (hex: Hex32): Hex32 => {
  require(HEX32.test(hex.toLowerCase()), `${hex} is not a 32-byte hex value`);
  return hex.toLowerCase();
};

/** Every daemon method of `@ome-labs/api`, by name. */
export const handlers: Handlers = {
  "daemon.info": async (d) => d.info(),
  "daemon.stop": async (d) => {
    setImmediate(() => void d.stop().then(() => process.emit("SIGTERM")));
    return null;
  },
  status: async (d) => d.status(),
  "profile.create": async (d, p) => d.createProfile(p),
  "profile.rename": async (d, p) => d.renameProfile(p.name),
  "profile.delete": async (d, p) => {
    d.deleteProfile(p.confirm);
    return null;
  },
  "config.get": async (d) => d.state.file.settings,
  "config.set": async (d, p) => {
    if (p.unlockTimeoutSecs !== undefined)
      require(p.unlockTimeoutSecs >= 60 &&
        p.unlockTimeoutSecs <= 86_400, "the unlock timeout is 60 to 86,400 seconds");
    if (p.nodeUrl !== undefined)
      require(/^https?:\/\//u.test(p.nodeUrl), "the node URL is http or https");
    await d.state.update((file) => {
      if (p.nodeUrl !== undefined) file.settings.nodeUrl = p.nodeUrl;
      if (p.proverEnabled !== undefined) file.settings.proverEnabled = p.proverEnabled;
      if (p.unlockTimeoutSecs !== undefined) file.settings.unlockTimeoutSecs = p.unlockTimeoutSecs;
      if (p.feePayerSalt !== undefined) file.settings.feePayerSalt = lower(p.feePayerSalt);
      if (p.aztecWalletDir !== undefined) file.settings.aztecWalletDir = p.aztecWalletDir || null;
      if (p.aztecWalletSync !== undefined) file.settings.aztecWalletSync = p.aztecWalletSync;
    });
    if (p.feePayerSalt !== undefined) await d.setFeePayer(d.state.file.settings.feePayerSalt);
    return d.state.file.settings;
  },
  "relays.set": async (d, p) => {
    require(p.relays.length <= 8 &&
      p.relays.every((r) =>
        RELAY.test(r),
      ), "relays are up to 8 multiaddrs ending in /p2p/<peer id>");
    await d.state.update((file) => {
      file.settings.relays = p.relays;
    });
    return d.state.file.settings;
  },
  "keeper.connect": async (d, p) => {
    if (p.remote) {
      require(/^[^\s:]+:[0-9]{1,5}$|^\[[0-9a-f:]+\]:[0-9]{1,5}$/iu.test(
        p.remote.address,
      ), "the keeper's address is host:port");
      require(/^[0-9a-f]{64}$/u.test(
        p.remote.fingerprint.toLowerCase().replaceAll(":", ""),
      ), "the fingerprint is 32 bytes of hex");
    }
    return d.connectKeeper(
      p.remote
        ? { ...p.remote, fingerprint: p.remote.fingerprint.toLowerCase().replaceAll(":", "") }
        : null,
    );
  },
  "signer.status": async (d) => d.signer.status(),
  "signer.unlock": async (d, p) => {
    await d.unlock(p.secret, p.ttlSecs);
    return d.signer.status();
  },
  "signer.lock": async (d) => {
    d.signer.lock();
    return d.signer.status();
  },
  "signer.sealingKey": async (d, p) => ({ key: d.signer.sealingKey(p.passphrase) }),
  "signer.changePassphrase": async (d, p) => d.changePassphrase(p.current, p.next),
  "node.status": async (d) => d.node.check(),
  "fees.status": async (d) => d.refreshFees(),

  "lobby.inspect": async (d, p) => d.founding.inspect(p.invite),
  "lobby.create": async (d, p) => d.founding.create(p),
  "lobby.join": async (d, p) => d.founding.join(p.invite),
  "lobby.get": async (d, p) => d.founding.get(p.lobby),
  "lobby.list": async (d) => d.founding.list(),
  "lobby.start": async (d, p) => d.founding.start(p),
  "lobby.remove": async (d, p) => d.founding.remove(p.lobby, lower(p.identity)),
  "lobby.update": async (d, p) => d.founding.update(p),
  "lobby.leave": async (d, p) => {
    await d.founding.leave(p.lobby);
    return null;
  },
  "founding.begin": async (d, p) => d.founding.begin(p.lobby),
  "founding.get": async (d, p) => d.founding.state(p.lobby),

  "vault.list": async (d) => vaultList(d),
  "vault.details": async (d, p) => d.vaults.details(lower(p.vault)),
  "vault.peers": async (d, p) => d.vaults.peers(lower(p.vault)),
  "vault.audit": async (d, p) => {
    // The keeper keeps its most recent entries; entries of the same second are told apart by their order in it.
    const entries = (await d.vaults.audit(lower(p.vault), AUDIT_WINDOW)).toReversed();
    const seen = new Map<number, number>();
    const keyed = entries.map((entry) => {
      const n = seen.get(entry.at) ?? 0;
      seen.set(entry.at, n + 1);
      return { entry, key: { at: entry.at, id: String(n).padStart(6, "0") } };
    });
    const result = page(keyed, (k) => k.key, p);
    return { items: result.items.map((k) => k.entry), next: result.next };
  },
  "vault.activity": async (d, p) => {
    const vault = lower(p.vault);
    const kinds = new Set(p.kinds ?? ["proposal", "received"]);
    const token = p.token === undefined ? undefined : lower(p.token);
    const [proposals, received] = await Promise.all([
      kinds.has("proposal") ? d.proposals.list(vault) : Promise.resolve([]),
      kinds.has("received") ? receivedTransfers(d, vault) : Promise.resolve([]),
    ]);
    const entries: ActivityEntry[] = [
      ...proposals
        .filter((proposal) => !token || proposal.token?.address === token)
        .map((proposal) => ({
          kind: "proposal" as const,
          at: proposal.execution?.at ?? proposal.createdAt,
          proposal,
        })),
      ...received.map((r) => r.entry).filter((entry) => !token || entry.token.address === token),
    ];
    return page(
      entries,
      (e) => ({
        at: e.at,
        id: e.kind === "proposal" ? e.proposal.id : `${e.txHash}:${e.token.address}`,
      }),
      p,
    );
  },
  "vault.counterparties": async (d, p) =>
    page(
      await counterparties(d, d.vaults.record(lower(p.vault)).address),
      (c) => ({ at: c.lastAt, id: c.address }),
      p,
    ),
  "vault.receiving": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    const received = await receivedTransfers(d, vault);
    return {
      handshakes: true,
      senders: d.senderCount,
      lastReceivedAt: received.at(-1)?.entry.at ?? null,
    };
  },
  "vault.request": async (d, p) => paymentRequest(d, { ...p, vault: lower(p.vault) }),
  "vault.rename": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    require(p.name.trim().length >= 1 && p.name.length <= 48, "a name is 1 to 48 characters");
    const renamed = await d.keeper.client.call("names.put", {
      vault,
      subject: "vault",
      name: p.name,
    });
    const event = await d.vaults.applyName(vault, renamed);
    if (event) d.emit(event);
    return d.vaults.summary(vault);
  },
  "vault.alias": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    require(ALIAS.test(p.alias), "aliases are lowercase letters, digits, - and _");
    await d.state.update((file) => {
      file.aliases[vault] = p.alias;
    });
    // With sync on, aztec-wallet can pay the vault by name too.
    if (d.state.file.settings.aztecWalletSync)
      await setAlias(walletDir(d.state.file.settings.aztecWalletDir), {
        key: `accounts:${p.alias}`,
        value: vault,
      });
    d.emit({ type: "vaultUpdated", vault });
    return vaultList(d);
  },
  "vault.retire": async (d, p) => {
    const record = d.vaults.record(lower(p.vault));
    // The member decides: one removed by a rotation its keeper took no part in cannot read the new config to show it.
    await d.keeper.client.call("vault.retire", { vault: record.address, force: true });
    record.status = "removed";
    await d.vaults.save(record);
    d.emit({ type: "vaultUpdated", vault: record.address });
    return vaultList(d);
  },
  "vault.use": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    await d.state.update((file) => {
      file.defaultVault = vault;
    });
    d.emit({ type: "vaultUpdated", vault });
    return vaultList(d);
  },

  "tokens.list": async (d, p) => balances(d, lower(p.vault)),
  "tokens.add": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    const info = await tokenInfo(d, vault, lower(p.token), p.deployment);
    await d.state.update((file) => {
      file.tokens[vault] = [
        ...(file.tokens[vault] ?? []).filter((t) => t.address !== info.address),
        info,
      ];
    });
    d.emit({ type: "tokenAdded", vault, token: info });
    return info;
  },
  "tokens.remove": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    const token = lower(p.token);
    await d.state.update((file) => {
      file.tokens[vault] = (file.tokens[vault] ?? []).filter((t) => t.address !== token);
    });
    d.emit({ type: "tokenRemoved", vault, token });
    return d.state.file.tokens[vault] ?? [];
  },
  "tokens.deployTest": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    const info = await deployTestToken(d, vault);
    await d.state.update((file) => {
      file.tokens[vault] = [...(file.tokens[vault] ?? []), info];
    });
    d.emit({ type: "tokenAdded", vault, token: info });
    d.emit({ type: "balanceChanged", vault, token: info.address });
    return info;
  },

  "proposals.list": async (d, p) => {
    const filter = p.filter ?? "all";
    const proposals = (await d.proposals.list(lower(p.vault))).filter((v) => matches(v, filter));
    return page(proposals, (v) => ({ at: v.createdAt, id: v.id }), p);
  },
  "proposals.get": async (d, p) => d.proposals.get({ vault: lower(p.vault), id: lower(p.id) }),
  "proposals.preview": async (d, p) => d.proposals.preview({ ...p, vault: lower(p.vault) }),
  "proposals.create": async (d, p) => d.proposals.create({ ...p, vault: lower(p.vault) }),
  "proposals.review": async (d, p) =>
    d.proposals.review({ vault: lower(p.vault), id: lower(p.id) }),
  "proposals.approve": async (d, p) =>
    d.proposals.approve({ ...p, vault: lower(p.vault), id: lower(p.id) }),
  "proposals.reject": async (d, p) => {
    require((p.reason ?? "").length <= 280, "a reason is at most 280 characters");
    return d.proposals.reject({ ...p, vault: lower(p.vault), id: lower(p.id) });
  },
  "proposals.execute": async (d, p) => {
    const view = await d.proposals.execute({
      vault: lower(p.vault),
      id: lower(p.id),
      force: p.force ?? false,
    });
    void d.refreshFees().catch(() => undefined);
    return view;
  },
  "proposals.cancel": async (d, p) =>
    d.proposals.cancel({ vault: lower(p.vault), id: lower(p.id) }),

  "contracts.functions": async (d, p) =>
    d.contracts.functions(lower(p.contract), p.includeUtility ?? false),
  "contracts.register": async (d, p) => {
    require(typeof p.artifact === "string" && p.artifact.length > 0, "give the artifact's JSON");
    return d.contracts.register({ ...p, address: lower(p.address) });
  },
  "contracts.list": async (d) => d.contracts.list(),
  "contracts.remove": async (d, p) => (await d.contracts.remove(lower(p.contract)), null),

  "aztec.aliases": async (d) => listAliases(walletDir(d.state.file.settings.aztecWalletDir)),
  "aztec.alias": async (d, p) => {
    require(d.state.file.settings
      .aztecWalletSync, "aztec-wallet sync is off: ome config set aztec-wallet.sync on");
    await setAlias(walletDir(d.state.file.settings.aztecWalletDir), p);
    await d.registerSenders();
    return null;
  },

  "membership.request": async (d) => d.membership.request(),
  "membership.plan": async (d, p) => d.membership.plan({ ...p, vault: lower(p.vault) }),
  "membership.begin": async (d, p) => d.membership.begin(p.plan),
  "membership.get": async (d, p) => d.membership.get(lower(p.vault), lower(p.id)),
  "membership.list": async (d, p) => {
    if (!d.vaults.list().some((r) => r.address === lower(p.vault)))
      throw new DaemonError("not-found", "unknown vault");
    return d.membership.list(lower(p.vault));
  },
  "membership.swapSigner": async (d, p) => {
    const vault = lower(p.vault);
    const id = await d.membership.swapSigner({ ...p, vault });
    return d.proposals.get({ vault, id });
  },
  "devices.pair": async (d) => d.devices.pair(),
  "devices.list": async (d) => d.devices.list(),
  "devices.revoke": async (d, p) => {
    await d.devices.revoke(lower(p.identity));
    return null;
  },
  "devices.enroll": async (d, p) => {
    const vault = lower(p.vault);
    const id = await d.devices.enroll({ vault, identity: lower(p.identity) });
    return d.proposals.get({ vault, id });
  },
  "devices.forward": async (d, p) => d.devices.forward({ vault: lower(p.vault), id: lower(p.id) }),
};
