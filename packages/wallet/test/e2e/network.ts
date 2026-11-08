// Local-network fixtures for the end-to-end suite.
import { getInitialTestAccountsData } from "@aztec-labs/accounts/testing";
import { SponsoredFeePaymentMethod } from "@aztec-labs/aztec.js/fee";
import { createAztecNodeClient } from "@aztec-labs/aztec.js/node";
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { SponsoredFPCContract } from "@aztec-labs/noir-contracts.js/SponsoredFPC";
import { getContractInstanceFromInstantiationParams } from "@aztec-labs/stdlib/contract";
import type { AztecNode } from "@aztec-labs/stdlib/interfaces/client";

import { OmeWallet } from "../../src/index.ts";

export const NODE_URL = process.env.AZTEC_NODE_URL ?? "http://localhost:8080";

/** `OME_E2E_PROVE=1` proves every transaction client-side (the local network accepts fake proofs otherwise). */
const PROVE = process.env.OME_E2E_PROVE === "1";

/** The genesis-funded SponsoredFPC (salt 0): it pays every fee, so the vault never pays its own. */
const sponsoredFpc = () =>
  getContractInstanceFromInstantiationParams(SponsoredFPCContract.artifact, { salt: new Fr(0n) });

/** Whether a local network answers at {@link NODE_URL}. */
export async function networkAvailable(): Promise<boolean> {
  try {
    await createAztecNodeClient(NODE_URL).getNodeInfo();
    return true;
  } catch {
    return false;
  }
}

export const node = (): AztecNode => createAztecNodeClient(NODE_URL);

/** A member's wallet with its own genesis test account and the sponsored fee payer registered. */
export async function memberWallet(aztecNode: AztecNode, testAccount: number) {
  const wallet = await OmeWallet.create(aztecNode, { pxe: { proverEnabled: PROVE } });
  const data = (await getInitialTestAccountsData())[testAccount];
  if (!data) throw new Error(`No test account ${testAccount}`);
  const account = await wallet.addSchnorrAccount(data.secret, data.salt, data.signingKey);
  const fpc = await sponsoredFpc();
  await wallet.registerContract(fpc, SponsoredFPCContract.artifact);
  return {
    wallet,
    account,
    feePayer: fpc.address,
    fee: { paymentMethod: new SponsoredFeePaymentMethod(fpc.address) },
  };
}

export type Member = Awaited<ReturnType<typeof memberWallet>>;
