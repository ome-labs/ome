import { Contract } from "@aztec-labs/aztec.js/contracts";
import { readFieldCompressedString } from "@aztec-labs/aztec.js/utils";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import type { ContractArtifact } from "@aztec-labs/stdlib/abi";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import {
  type ContractInstanceWithAddress,
  getContractClassFromArtifact,
  getContractInstanceFromInstantiationParams,
} from "@aztec-labs/stdlib/contract";
import type { BalanceState, Hex32, TokenBalance, TokenDeployment, TokenInfo } from "@ome-labs/api";

import type { Context } from "../context.ts";
import { DaemonError } from "../errors.ts";

/** The local network's L1 chain id: test tokens are deployed only there. */
const LOCAL_CHAIN_ID = 31_337;

/**
 * The token's instance: from its deployment when given (checked to derive the token's address), else as the node
 * publishes it.
 */
async function tokenInstance(
  context: Context,
  address: AztecAddress,
  deployment: TokenDeployment | undefined,
): Promise<ContractInstanceWithAddress> {
  if (deployment) {
    const instance = await getContractInstanceFromInstantiationParams(TokenContract.artifact, {
      salt: Fr.fromString(deployment.salt),
      deployer: AztecAddress.fromStringUnsafe(deployment.deployer),
      constructorArgs: [
        AztecAddress.fromStringUnsafe(deployment.admin),
        deployment.name,
        deployment.symbol,
        deployment.decimals,
      ],
    });
    if (!instance.address.equals(address))
      throw new DaemonError("invalid", "this deployment derives another address than the token's");
    return instance;
  }
  const instance = await context.node.node.getContract(address);
  if (!instance)
    throw new DaemonError(
      "not-found",
      `no published token at ${address.toString()}; pass its deployment to add it`,
    );
  return instance;
}

let standardClassId: Promise<Fr> | undefined;

/** The class of Aztec's standard `Token` contract. */
function standardTokenClass(): Promise<Fr> {
  standardClassId ??= getContractClassFromArtifact(TokenContract.artifact).then((c) => c.id);
  return standardClassId;
}

/** What Ome calls on a token: the balance it reads, the transfer it proposes, and the metadata getters. */
const TOKEN_FUNCTIONS = ["balance_of_private", "transfer"] as const;
const METADATA = ["name", "symbol", "decimals"] as const;

/** The getter a token has for `field`: Aztec's `private_get_…`, else `public_get_…`. */
function getter(artifact: ContractArtifact, field: (typeof METADATA)[number]): string | undefined {
  return [`private_get_${field}`, `public_get_${field}`].find((name) =>
    artifact.functions.some((f) => f.name === name),
  );
}

/**
 * The token's code: Aztec's standard `Token`, or a contract with its own code that this member registered
 * (`contracts.register`) and that has the functions Ome calls. A missing function is refused by name.
 */
async function tokenArtifact(
  context: Context,
  token: Hex32,
  instance: ContractInstanceWithAddress,
): Promise<{ artifact: ContractArtifact; standard: boolean }> {
  if (instance.currentContractClassId.equals(await standardTokenClass()))
    return { artifact: TokenContract.artifact, standard: true };
  const { artifact } = await context.contracts.artifact(token).catch(() => {
    throw new DaemonError(
      "invalid",
      `${token} isn't Aztec's standard token: register its artifact first (contracts.register)`,
    );
  });
  const missing = [
    ...TOKEN_FUNCTIONS.filter((name) => !artifact.functions.some((f) => f.name === name)),
    ...METADATA.filter((field) => !getter(artifact, field)).map((field) => `a ${field} getter`),
  ];
  if (missing.length > 0)
    throw new DaemonError(
      "invalid",
      `${token} isn't a token Ome can use: it has no ${missing.join(", ")}`,
    );
  return { artifact, standard: false };
}

/**
 * A token's name, symbol and decimals. They are public, so they are read from the member's own account: a private call
 * as the vault would need an approved proposal. The token's instance is registered in the member's PXE first, so a
 * member who did not deploy it can still read and transfer it: the published one, or the one its deployment derives.
 * A token with its own code is read with the artifact this member registered for it.
 */
export async function tokenInfo(
  context: Context,
  vault: Hex32,
  token: Hex32,
  deployment?: TokenDeployment,
): Promise<TokenInfo> {
  context.vaults.record(vault);
  const from = context.account();
  const address = AztecAddress.fromStringUnsafe(token);
  const instance = await tokenInstance(context, address, deployment);
  const { artifact, standard } = await tokenArtifact(context, token, instance);
  if (standard) await context.wallet().registerContract(instance, artifact);
  const contract = Contract.at(address, artifact, context.wallet());
  const read = async (field: (typeof METADATA)[number]) => {
    const name = getter(artifact, field) ?? `private_get_${field}`;
    return (await contract.methods[name]?.().simulate({ from }))?.result as unknown;
  };
  const [name, symbol, decimals] = await Promise.all([
    read("name"),
    read("symbol"),
    read("decimals"),
  ]);
  return {
    address: token,
    name: readFieldCompressedString(name as never),
    symbol: readFieldCompressedString(symbol as never),
    decimals: Number(decimals),
    standard,
  };
}

/** The amount of a balance state, when known. */
export const known = (state: BalanceState): string | null =>
  state.state === "known" ? state.amount : null;

/** Keepers that answered and were needed, from an error the keeper raised, however Aztec wrapped it. */
function unreadable(error: unknown): { answered: number; needed: number } | undefined {
  for (let e: unknown = error; e; e = (e as { cause?: unknown }).cause) {
    const details = (e as { details?: { kind?: string; answered?: number; needed?: number } })
      .details;
    if (
      details?.kind === "unreadable" &&
      details.answered !== undefined &&
      details.needed !== undefined
    )
      return { answered: details.answered, needed: details.needed };
    const text = e instanceof Error ? e.message : String(e);
    const match = /only (\d+) of (\d+) members answered/u.exec(text);
    if (match) return { answered: Number(match[1]), needed: Number(match[2]) };
    if (!(e instanceof Error)) break;
  }
  return undefined;
}

/** A read as a balance state: the amount, too few keepers (from the keeper's error, however Aztec wrapped it), or the error verbatim. */
export async function balanceState(read: () => Promise<unknown>): Promise<BalanceState> {
  try {
    return { state: "known", amount: String(await read()) };
  } catch (error) {
    const short = unreadable(error);
    if (short) return { state: "unreadable", ...short };
    return { state: "failed", message: error instanceof Error ? error.message : String(error) };
  }
}

/**
 * Refuses with `insufficient-funds` when the vault is known to hold less of `token` than `amount`. A token the member
 * doesn't track, or a balance that can't be read now, is no reason to refuse: the simulation decides then.
 */
export async function requireBalance(
  context: Context,
  vault: Hex32,
  token: Hex32,
  amount: string,
): Promise<void> {
  const info = (context.state.file.tokens[vault] ?? []).find((t) => t.address === token);
  if (!info) return;
  const [read] = await balances(context, vault, [info], { public: false });
  const held = read ? known(read.private) : null;
  if (held === null) return;
  if (BigInt(held) >= BigInt(amount)) return;
  throw DaemonError.detailed(
    {
      kind: "insufficient-funds",
      funds: { type: "token", token: info },
      needed: amount,
      held,
    },
    `the vault holds ${held} base units of ${info.symbol}; the transfer needs ${amount}`,
  );
}

/**
 * The vault's balance of each token the member tracks: private (its notes, through the keepers) and public (the
 * token's storage, read from the member's own account, so it shows even when the keepers can't answer). A token
 * without `balance_of_public` reports why.
 */
export async function balances(
  context: Context,
  vault: Hex32,
  tokens: readonly TokenInfo[] = context.state.file.tokens[vault] ?? [],
  read: { public: boolean } = { public: true },
): Promise<TokenBalance[]> {
  const owner = context.vaults.address(vault);
  const me = context.account();
  return Promise.all(
    tokens.map(async (token) => {
      // The standard artifact builds the calls; the PXE runs the code registered at the address.
      const contract = TokenContract.at(
        AztecAddress.fromStringUnsafe(token.address),
        context.wallet(),
      );
      const [privateState, publicState] = await Promise.all([
        balanceState(
          async () =>
            (await contract.methods.balance_of_private(owner).simulate({ from: owner })).result,
        ),
        read.public
          ? balanceState(
              async () =>
                (await contract.methods.balance_of_public(owner).simulate({ from: me })).result,
            )
          : Promise.resolve<BalanceState>({ state: "failed", message: "not read" }),
      ]);
      return { token, private: privateState, public: publicState };
    }),
  );
}

/** Deploys a test token from the member's account and mints 1,000 whole tokens to the vault (local network only). */
export async function deployTestToken(context: Context, vault: Hex32): Promise<TokenInfo> {
  if (context.node.status.l1ChainId !== LOCAL_CHAIN_ID)
    throw DaemonError.detailed(
      { kind: "network", expected: "local", actual: context.node.network() },
      "test tokens are only for the local network",
    );
  const from = context.account();
  const fee = context.fee();
  const { contract } = await TokenContract.deploy(
    context.wallet(),
    from,
    "Ome Test",
    "OMET",
    18,
  ).send({ from, fee });
  await contract.methods
    .mint_to_private(context.vaults.address(vault), 1000n * 10n ** 18n)
    .send({ from, fee });
  return {
    address: contract.address.toString(),
    name: "Ome Test",
    symbol: "OMET",
    decimals: 18,
    standard: true,
  };
}
