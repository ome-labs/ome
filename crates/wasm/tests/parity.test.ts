// WebAssembly results equal the golden vectors the Rust crates are tested against, which come from Aztec 6.0.0's own
// packages (tools/vectors).
import { readFileSync } from "node:fs";
import { join } from "node:path";

import type { Hex32, Point } from "@ome-labs/api";
import {
  type Call,
  addressContext,
  appPayloadHash,
  approvalMessage,
  committeeCommitment,
  outerHash,
  preaddress,
  selector,
  signerSetCommitment,
  verifyAndCombine,
} from "@ome-labs/wasm";
import { dealShares, partials } from "@ome-labs/wasm/testing";
import { describe, expect, it } from "vitest";

import { at } from "./fixture.ts";

const crates = join(import.meta.dirname, "..", "..");
const load = <T>(path: string): T => JSON.parse(readFileSync(join(crates, path), "utf8")) as T;

interface ContractVectors {
  signerSets: { count: number; keys: Point[]; commitment: Hex32 }[];
  committees: {
    viewingThreshold: number;
    memberCount: number;
    shares: Point[];
    commitment: Hex32;
  }[];
  approvalMessages: {
    vault: Hex32;
    outerHash: Hex32;
    configVersion: number;
    expiresAt: string;
    message: Hex32;
  }[];
}

interface VectorCall {
  signature: string;
  to: Hex32;
  type: "private" | "public";
  isStatic: boolean;
  hideMsgSender: boolean;
  args: Hex32[];
}

interface PayloadVectors {
  context: { vault: Hex32; chainId: Hex32; version: Hex32 };
  selectors: { signature: string; selector: string }[];
  payloads: Record<
    string,
    { calls: VectorCall[]; txNonce: Hex32; payloadHash: Hex32; outerHash: Hex32 }
  >;
}

interface AccountVectors {
  accounts: {
    ivsk: Hex32;
    ivpk: Point;
    publicKeysHash: Hex32;
    partialAddress: Hex32;
    preaddress: Hex32;
    address: Hex32;
    ecdh: { ephPk: Point; sharedPoint: Point };
  }[];
}

const contract = load<ContractVectors>("protocol/tests/vectors/contract.json");
const payload = load<PayloadVectors>("protocol/tests/vectors/payload.json");
const aztec = load<AccountVectors>("crypto/tests/vectors/aztec.json");

const xy = ({ x, y }: Point): Point => ({ x, y });

describe("contract mirror", () => {
  // The vectors list keys and shares padded to the contract's eight slots; the bindings take the real ones.
  it("matches the signer-set and committee commitments", () => {
    for (const set of contract.signerSets) {
      expect(signerSetCommitment(set.keys.slice(0, set.count).map(xy))).toBe(set.commitment);
    }
    for (const committee of contract.committees) {
      const shares = committee.shares.slice(0, committee.memberCount).map(xy);
      expect(committeeCommitment(committee.viewingThreshold, shares)).toBe(committee.commitment);
    }
  });

  // Times are JavaScript numbers, so a deadline past 2^53 (one vector uses u64::MAX) cannot be expressed.
  it("matches the approval messages", () => {
    const representable = contract.approvalMessages.filter(
      (v) => BigInt(v.expiresAt) <= BigInt(Number.MAX_SAFE_INTEGER),
    );
    expect(representable.length).toBeGreaterThan(0);
    for (const v of representable) {
      expect(approvalMessage(v.vault, v.outerHash, v.configVersion, Number(v.expiresAt))).toBe(
        v.message,
      );
    }
  });
});

const call = (c: VectorCall): Call => ({
  signature: c.signature,
  to: c.to,
  selector: selector(c.signature),
  kind: c.type,
  isStatic: c.isStatic,
  hideMsgSender: c.hideMsgSender,
  args: c.args,
});

describe("payloads", () => {
  it("matches the selectors", () => {
    for (const v of payload.selectors) {
      expect(selector(v.signature)).toBe(Number.parseInt(v.selector, 16));
    }
  });

  it("matches the payload and outer hashes", () => {
    const { vault, chainId, version } = payload.context;
    for (const v of Object.values(payload.payloads)) {
      const calls = v.calls.map(call);
      expect(appPayloadHash(calls, v.txNonce)).toBe(v.payloadHash);
      expect(outerHash(vault, { chainId, version }, v.txNonce, calls)).toBe(v.outerHash);
    }
  });
});

describe("threshold reading", () => {
  it("matches Aztec's addresses, and combining dealt shares gives Aztec's ECDH point", () => {
    for (const account of aztec.accounts) {
      const ivpk = xy(account.ivpk);
      expect(preaddress(account.publicKeysHash, account.partialAddress)).toBe(account.preaddress);
      expect(addressContext(account.preaddress, ivpk).address).toBe(account.address);

      const committee = dealShares(account.ivsk, 2, 3);
      expect(committee.groupKey).toEqual(ivpk);
      const inputs = [xy(account.ecdh.ephPk)];
      const responses = [0, 2].map((i) =>
        partials(i + 1, at(committee.shares, i), account.preaddress, ivpk, 0, inputs),
      );
      const result = verifyAndCombine({
        epoch: 0,
        preaddress: account.preaddress,
        ivpk,
        verificationShares: committee.verificationShares,
        threshold: 2,
        inputs,
        responses,
      });
      expect(result.points).toEqual([xy(account.ecdh.sharedPoint)]);
    }
  });
});
