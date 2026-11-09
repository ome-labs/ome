import { readFieldCompressedString } from "@aztec-labs/aztec.js/utils";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import {
  type ContractInstanceWithAddress,
  getContractInstanceFromInstantiationParams,
} from "@aztec-labs/stdlib/contract";
import type { Hex32, TokenBalance, TokenDeployment, TokenInfo } from "@ome-labs/api";

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

/**
 * A token's name, symbol and decimals. They are public, so they are read from the member's own account: a private call
 * as the vault would need an approved proposal. The token's instance is registered in the member's PXE first, so a
 * member who did not deploy it can still read and transfer it: the published one, or the one its deployment derives.
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
  await context.wallet().registerContract(instance, TokenContract.artifact);
  const contract = TokenContract.at(address, context.wallet());
  const read = async (method: "private_get_name" | "private_get_symbol" | "private_get_decimals") =>
    (await contract.methods[method]().simulate({ from })).result as unknown;
  const [name, symbol, decimals] = await Promise.all([
    read("private_get_name"),
    read("private_get_symbol"),
    read("private_get_decimals"),
  ]);
  return {
    address: token,
    name: readFieldCompressedString(name as never),
    symbol: readFieldCompressedString(symbol as never),
    decimals: Number(decimals),
  };
}

/** The vault's private balance of each token the member tracks. */
export async function balances(context: Context, vault: Hex32): Promise<TokenBalance[]> {
  const from = context.vaults.address(vault);
  const tokens = context.state.file.tokens[vault] ?? [];
  return Promise.all(
    tokens.map(async (token) => {
      try {
        const { result } = await TokenContract.at(
          AztecAddress.fromStringUnsafe(token.address),
          context.wallet(),
        )
          .methods.balance_of_private(from)
          .simulate({ from });
        return { token, balance: String(result), error: null };
      } catch (error) {
        return {
          token,
          balance: null,
          error: error instanceof Error ? error.message : String(error),
        };
      }
    }),
  );
}

/** Deploys a test token from the member's account and mints 1,000 whole tokens to the vault (local network only). */
export async function deployTestToken(context: Context, vault: Hex32): Promise<TokenInfo> {
  if (context.node.status.l1ChainId !== LOCAL_CHAIN_ID)
    throw new DaemonError("invalid", "test tokens are only for the local network");
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
  return { address: contract.address.toString(), name: "Ome Test", symbol: "OMET", decimals: 18 };
}
