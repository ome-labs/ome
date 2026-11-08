// Key derivation, preaddress, address, address secret, ECDH and app siloing for deterministic accounts, covering both
// branches of Aztec's address-secret negation rule.

import { Fq, Fr } from "@aztec-labs/foundation/curves/bn254";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import {
  computeAddress,
  computeAddressSecret,
  computePreaddress,
  deriveKeys,
  derivePublicKeyFromSecretKey,
} from "@aztec-labs/stdlib/keys";
import { appSiloEcdhSharedSecretPoint, deriveEcdhSharedSecretPoint } from "@aztec-labs/stdlib/logs";

import { hex, point } from "../encoding.mjs";

/** Accounts kept per sign of the address secret. */
const PER_SIGN = 3;

/** Seeds tried, in order, until both signs have `PER_SIGN` accounts. */
const CANDIDATE_SEEDS = 40;

const APP = new AztecAddress(new Fr(0xc0ffeen));

async function account(seed) {
  const secretKey = new Fr(seed * 0x1234567n);
  const partialAddress = new Fr(seed * 0x9abcdefn + 11n);
  const { masterIncomingViewingSecretKey: ivsk, publicKeys } = await deriveKeys(secretKey);

  const publicKeysHash = await publicKeys.hash();
  const preaddress = await computePreaddress(publicKeysHash, partialAddress);
  const addressSecret = await computeAddressSecret(preaddress, ivsk);

  const ephSecret = new Fq(seed * 0x55555n + 3n);
  const ephPk = await derivePublicKeyFromSecretKey(ephSecret);
  const sharedPoint = await deriveEcdhSharedSecretPoint(addressSecret, ephPk);

  return {
    ivsk: hex(ivsk),
    ivpk: point(publicKeys.ivpkM),
    publicKeysHash: hex(publicKeysHash),
    partialAddress: hex(partialAddress),
    preaddress: hex(preaddress),
    address: hex(await computeAddress(publicKeys, partialAddress)),
    addressSecret: hex(addressSecret),
    addressSecretNegated:
      addressSecret.toBigInt() !== ivsk.add(new Fq(preaddress.toBigInt())).toBigInt(),
    addressPoint: point(await derivePublicKeyFromSecretKey(addressSecret)),
    ecdh: {
      ephSecret: hex(ephSecret),
      ephPk: point(ephPk),
      sharedPoint: point(sharedPoint),
      app: hex(APP),
      appSiloedSecret: hex(await appSiloEcdhSharedSecretPoint(sharedPoint, APP)),
    },
  };
}

/** The first `PER_SIGN` plain accounts, then the first `PER_SIGN` negated ones, in seed order. */
export async function accountVectors() {
  const candidates = await Promise.all(
    Array.from({ length: CANDIDATE_SEEDS }, (_, i) => account(BigInt(i + 1))),
  );
  const plain = candidates.filter((a) => !a.addressSecretNegated).slice(0, PER_SIGN);
  const negated = candidates.filter((a) => a.addressSecretNegated).slice(0, PER_SIGN);

  if (plain.length < PER_SIGN || negated.length < PER_SIGN) {
    throw new Error("not enough candidate accounts to cover both address-secret signs");
  }

  return [...plain, ...negated];
}
