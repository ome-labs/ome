// Field moduli, the Grumpkin generator and fixed-base scalar multiplication, including the edge scalars.

import { DomainSeparator } from "@aztec-labs/constants";
import { Grumpkin } from "@aztec-labs/foundation/crypto/grumpkin";
import { Fq, Fr } from "@aztec-labs/foundation/curves/bn254";
import { derivePublicKeyFromSecretKey } from "@aztec-labs/stdlib/keys";

import { type EncodedPoint, hex, point } from "../encoding.ts";

const SCALARS = [1n, 2n, 7n, 0xdeadbeefn, Fq.MODULUS - 1n, (Fq.MODULUS - 1n) / 2n + 3n];

export interface CurveVectors {
  frModulus: string;
  fqModulus: string;
  domainSeparators: { contractAddressV3: number; appSiloedEcdhSharedSecret: number };
  generator: EncodedPoint;
  scalarMuls: { scalar: string; result: EncodedPoint }[];
}

export async function curveVectors(): Promise<CurveVectors> {
  const scalarMuls = await Promise.all(
    SCALARS.map(async (value) => {
      const scalar = new Fq(value);
      return { scalar: hex(scalar), result: point(await derivePublicKeyFromSecretKey(scalar)) };
    }),
  );

  return {
    frModulus: `0x${Fr.MODULUS.toString(16)}`,
    fqModulus: `0x${Fq.MODULUS.toString(16)}`,
    domainSeparators: {
      contractAddressV3: DomainSeparator.CONTRACT_ADDRESS_V3,
      appSiloedEcdhSharedSecret: DomainSeparator.APP_SILOED_ECDH_SHARED_SECRET,
    },
    generator: point(Grumpkin.generator),
    scalarMuls,
  };
}
