import { FunctionType } from "@aztec-labs/stdlib/abi";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import type {
  ActivityEntry,
  DaemonMethod,
  DaemonParams,
  DaemonResult,
  Hex32,
  VaultSummary,
} from "@ome-labs/api";

import type { Daemon } from "../daemon.ts";
import { DaemonError, require } from "../errors.ts";
import { artifactOf } from "../proposals/intents.ts";
import { balances, deployTestToken, tokenInfo } from "../vaults/tokens.ts";

type Served = Exclude<DaemonMethod, "session.auth" | "events.subscribe">;
type Handlers = {
  [M in Served]: (daemon: Daemon, params: DaemonParams<M>) => Promise<DaemonResult<M>>;
};

const ALIAS = /^[a-z0-9][a-z0-9_-]{0,31}$/u;
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
    });
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
  "node.status": async (d) => d.node.check(),

  "lobby.inspect": async (d, p) => d.founding.inspect(p.invite),
  "lobby.create": async (d, p) => d.founding.create(p),
  "lobby.join": async (d, p) => d.founding.join(p.invite),
  "lobby.get": async (d, p) => d.founding.get(p.lobby),
  "lobby.list": async (d) => d.founding.list(),
  "lobby.start": async (d, p) => d.founding.start(p),
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
    require(p.limit >= 1 && p.limit <= 500, "the limit is 1 to 500");
    return d.vaults.audit(lower(p.vault), p.limit);
  },
  "vault.activity": async (d, p) => {
    const proposals = await d.proposals.list(lower(p.vault));
    const entries: ActivityEntry[] = proposals.map((proposal) => ({
      kind: "proposal",
      at: proposal.execution?.at ?? proposal.expiresAt,
      proposal,
    }));
    return entries.toSorted((a, b) => b.at - a.at);
  },
  "vault.alias": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    require(ALIAS.test(p.alias), "aliases are lowercase letters, digits, - and _");
    await d.state.update((file) => {
      file.aliases[vault] = p.alias;
    });
    return vaultList(d);
  },
  "vault.retire": async (d, p) => {
    const record = d.vaults.record(lower(p.vault));
    await d.keeper.client.call("vault.retire", { vault: record.address });
    record.status = "removed";
    await d.vaults.save(record);
    return vaultList(d);
  },
  "vault.use": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    await d.state.update((file) => {
      file.defaultVault = vault;
    });
    return vaultList(d);
  },

  "tokens.list": async (d, p) => balances(d, lower(p.vault)),
  "tokens.add": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    const info = await tokenInfo(d, vault, lower(p.token));
    await d.state.update((file) => {
      file.tokens[vault] = [
        ...(file.tokens[vault] ?? []).filter((t) => t.address !== info.address),
        info,
      ];
    });
    return info;
  },
  "tokens.deployTest": async (d, p) => {
    const vault = d.vaults.record(lower(p.vault)).address;
    const info = await deployTestToken(d, vault);
    await d.state.update((file) => {
      file.tokens[vault] = [...(file.tokens[vault] ?? []), info];
    });
    return info;
  },

  "proposals.list": async (d, p) => d.proposals.list(lower(p.vault)),
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
  "proposals.execute": async (d, p) =>
    d.proposals.execute({ vault: lower(p.vault), id: lower(p.id) }),
  "proposals.cancel": async (d, p) =>
    d.proposals.cancel({ vault: lower(p.vault), id: lower(p.id) }),

  "contracts.functions": async (d, p) => {
    const artifact = await artifactOf(d, AztecAddress.fromStringUnsafe(lower(p.contract)));
    return artifact.functions
      .filter((f) => !f.isOnlySelf && !f.isInitializer)
      .map((f) => ({
        name: f.name,
        kind:
          f.functionType === FunctionType.PRIVATE
            ? ("private" as const)
            : f.functionType === FunctionType.PUBLIC
              ? ("public" as const)
              : ("utility" as const),
        isStatic: f.isStatic,
        params: f.parameters.map((param) => ({ name: param.name, type: param.type.kind })),
      }));
  },

  "contacts.list": async (d) => d.state.file.contacts,
  "contacts.put": async (d, p) => {
    const address = lower(p.address);
    require(ALIAS.test(p.label), "labels are lowercase letters, digits, - and _");
    await d.state.update((file) => {
      file.contacts = [
        ...file.contacts.filter((c) => c.address !== address),
        { address, label: p.label },
      ];
    });
    return d.state.file.contacts;
  },
  "contacts.remove": async (d, p) => {
    const address = lower(p.address);
    await d.state.update((file) => {
      file.contacts = file.contacts.filter((c) => c.address !== address);
    });
    return d.state.file.contacts;
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
};
