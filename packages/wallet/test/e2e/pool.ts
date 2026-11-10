// A token pair and an AMM with liquidity on a local network, as an app's world would have them, for suites that then
// trade against it through Ome alone (`scripts/test-cli.ts`). A genesis test account deploys everything and provides
// the liquidity; the vault gets 1,000 of the first token.
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { AMMContract } from "@aztec-labs/noir-contracts.js/AMM";
import { TokenContract } from "@aztec-labs/noir-contracts.js/Token";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";

import { memberWallet, node } from "./network.ts";

export interface Pool {
  /** The AMM, and the compiled artifact a member registers it with. */
  readonly amm: string;
  readonly artifact: string;
  /** The pair: the vault holds 1,000 JADE. */
  readonly jade: string;
  readonly opal: string;
}

export async function deployPool(vault: string): Promise<Pool> {
  const { wallet, account, fee } = await memberWallet(node(), 0);
  try {
    const send = { from: account, fee };
    const deploy = async (name: string, symbol: string) =>
      (await TokenContract.deploy(wallet, account, name, symbol, 18).send(send)).contract;
    const jade = await deploy("Jade", "JADE");
    const opal = await deploy("Opal", "OPAL");
    const shares = await deploy("Ome LP", "OLP");
    const { contract: amm } = await AMMContract.deploy(
      wallet,
      jade.address,
      opal.address,
      shares.address,
    ).send(send);
    await shares.methods.set_minter(amm.address, true).send(send);
    await jade.methods.mint_to_private(account, 10_000n).send(send);
    await opal.methods.mint_to_private(account, 10_000n).send(send);
    await jade.methods.mint_to_private(AztecAddress.fromStringUnsafe(vault), 1_000n).send(send);
    const nonce = Fr.random();
    const authWitnesses = await Promise.all(
      [jade, opal].map(async (token) =>
        wallet.createAuthWit(account, {
          caller: amm.address,
          call: await token.methods
            .transfer_to_public_and_prepare_private_balance_increase(
              account,
              amm.address,
              10_000n,
              nonce,
            )
            .getFunctionCall(),
        }),
      ),
    );
    await amm.methods
      .add_liquidity(10_000n, 10_000n, 9_000n, 9_000n, nonce)
      .with({ authWitnesses })
      .send(send);
    const dest = dirname(fileURLToPath(import.meta.resolve("@aztec-labs/noir-contracts.js/AMM")));
    return {
      amm: amm.address.toString(),
      artifact: join(dest, "..", "artifacts", "amm_contract-AMM.json"),
      jade: jade.address.toString(),
      opal: opal.address.toString(),
    };
  } finally {
    await wallet.stop();
  }
}
