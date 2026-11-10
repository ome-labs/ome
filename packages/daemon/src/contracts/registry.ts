// The contracts a member can call: those it registered, the vault's tracked tokens and the vault itself. Registering
// checks the artifact is the code of the class at the address; a proposal's contracts travel to the other members
// through the keepers, and each member's daemon checks them the same way before using them.
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { gunzipSync, gzipSync } from "node:zlib";

import { Fr } from "@aztec-labs/foundation/curves/bn254";
import {
  type ContractArtifact,
  FunctionSelector,
  FunctionType,
  contractArtifactFromBuffer,
  contractArtifactToBuffer,
} from "@aztec-labs/stdlib/abi";
import { AztecAddress } from "@aztec-labs/stdlib/aztec-address";
import {
  type ContractInstancePreimageWithAddress,
  SerializableContractInstancePreimage,
  computeContractAddressFromInstance,
} from "@aztec-labs/stdlib/contract";
import { PublicKeys } from "@aztec-labs/stdlib/keys";
import type {
  ContractFunction,
  ContractRegisterParams,
  ContractView,
  Hex32,
  HexBytes,
} from "@ome-labs/api";
import type { PayloadContracts } from "@ome-labs/wallet";

import type { Context } from "../context.ts";
import { DaemonError, notReady } from "../errors.ts";
import { privateDir, writePrivate } from "../profile/files.ts";
import { apiType } from "./abi.ts";
import { classIdOf, pack, parseArtifact, unpack } from "./artifacts.ts";

const lower = (value: { toString(): string }) => value.toString().toLowerCase() as Hex32;
const hex = (bytes: Uint8Array): HexBytes => `0x${Buffer.from(bytes).toString("hex")}`;

/** Functions a proposal can call: not utility, not only-self, not the initializer. */
function callable(artifact: ContractArtifact, includeUtility: boolean) {
  return artifact.functions.filter(
    (f) =>
      !f.isOnlySelf &&
      !f.isInitializer &&
      (includeUtility || f.functionType !== FunctionType.UTILITY),
  );
}

export class ContractRegistry {
  readonly #context: Context;

  constructor(context: Context) {
    this.#context = context;
  }

  /** Registers the member's contracts with the PXE again after a start. One that no longer loads is skipped. */
  async load(): Promise<void> {
    for (const [address, record] of Object.entries(this.#context.state.file.contracts)) {
      // oxlint-disable-next-line no-await-in-loop
      await this.#loadOne(address as Hex32, record.classId, record.instance).catch(() => undefined);
    }
  }

  async #loadOne(address: Hex32, classId: Hex32, instance: HexBytes | null): Promise<void> {
    const artifact = await this.#saved(classId);
    if (!artifact) return;
    await this.#context
      .wallet()
      .registerContract(await this.#instance(address, classId, instance), artifact);
  }

  /** Checks `artifact` is the code of the class at `address`, then registers it, keeps it, and offers it to members. */
  async register(params: ContractRegisterParams): Promise<ContractView> {
    const artifact = parseArtifact(params.artifact);
    const classId = await classIdOf(artifact);
    const address = lower(params.address);
    const given = params.instance
      ? await this.#givenInstance(address, classId, params.instance)
      : null;
    const instance = given ?? (await this.#instance(address, classId, null));
    await this.#context.wallet().registerContract(instance, artifact);
    await this.#save(classId, artifact);
    await this.#context.state.update((file) => {
      file.contracts[address] = {
        name: artifact.name,
        classId,
        instance: given ? hex(new SerializableContractInstancePreimage(given).toBuffer()) : null,
      };
    });
    await this.#offer(classId, artifact).catch(() => undefined);
    return {
      address,
      name: artifact.name,
      classId,
      source: "registered",
      functions: callable(artifact, false).length,
    };
  }

  remove(address: Hex32): Promise<void> {
    return this.#context.state.update((file) => {
      delete file.contracts[lower(address)];
    });
  }

  /** The member's registered contracts, every vault's tracked tokens, and the vaults. */
  async list(): Promise<ContractView[]> {
    const { contracts, tokens } = this.#context.state.file;
    const entries: { address: Hex32; source: ContractView["source"] }[] = [
      ...Object.keys(contracts).map((address) => ({
        address: address as Hex32,
        source: "registered" as const,
      })),
      ...Object.values(tokens)
        .flat()
        .map((token) => ({ address: token.address, source: "token" as const })),
      ...this.#context.vaults
        .list()
        .map((record) => ({ address: record.address, source: "vault" as const })),
    ];
    const seen = new Set<Hex32>();
    const views = await Promise.all(
      entries
        .filter(({ address }) => !seen.has(address) && seen.add(address))
        .map(async ({ address, source }) => {
          const found = await this.#artifactAt(address).catch(() => undefined);
          if (!found) return undefined;
          const view: ContractView = {
            address,
            name: found.artifact.name,
            classId: found.classId,
            source,
            functions: callable(found.artifact, false).length,
          };
          return view;
        }),
    );
    return views.filter((view): view is ContractView => view !== undefined);
  }

  /** A contract's functions with their full types. */
  async functions(address: Hex32, includeUtility: boolean): Promise<ContractFunction[]> {
    const { artifact } = await this.artifact(address);
    return Promise.all(
      callable(artifact, includeUtility).map(async (f) => ({
        name: f.name,
        selector: (await FunctionSelector.fromNameAndParameters(f.name, f.parameters)).toString(),
        kind:
          f.functionType === FunctionType.PRIVATE
            ? ("private" as const)
            : f.functionType === FunctionType.PUBLIC
              ? ("public" as const)
              : ("utility" as const),
        isStatic: f.isStatic,
        params: f.parameters.map((p) => ({ name: p.name, type: apiType(p.type) })),
        returns: (f.returnTypes ?? []).map(apiType),
      })),
    );
  }

  /** The artifact of the contract at `address`, which this member's PXE must know. */
  async artifact(address: Hex32): Promise<{ artifact: ContractArtifact; classId: Hex32 }> {
    const found = await this.#artifactAt(address);
    if (found) return found;
    const instance = await this.#context
      .wallet()
      .pxeService.getContractInstance(AztecAddress.fromStringUnsafe(address));
    throw DaemonError.detailed(
      {
        kind: "unknown-contract",
        address,
        ...(instance ? { classId: lower(instance.originalContractClassId) } : {}),
      },
      `contract ${address} is not registered with this member`,
    );
  }

  async #artifactAt(
    address: Hex32,
  ): Promise<{ artifact: ContractArtifact; classId: Hex32 } | undefined> {
    const pxe = this.#context.wallet().pxeService;
    const instance = await pxe.getContractInstance(AztecAddress.fromStringUnsafe(address));
    if (!instance) return undefined;
    const artifact = await pxe.getContractArtifact(instance.originalContractClassId);
    return artifact ? { artifact, classId: lower(instance.originalContractClassId) } : undefined;
  }

  /**
   * What other members need to run `targets`: their classes, and the instances the node doesn't publish. The artifacts
   * of those classes are offered to this member's keeper, so members' keepers can fetch them.
   */
  async forProposal(targets: AztecAddress[]): Promise<PayloadContracts> {
    const pxe = this.#context.wallet().pxeService;
    const classIds = new Set<Hex32>();
    const instances: PayloadContracts["instances"][number][] = [];
    for (const target of new Set(targets.map((t) => t.toString()))) {
      const address = AztecAddress.fromStringUnsafe(target);
      // oxlint-disable-next-line no-await-in-loop
      const instance = await pxe.getContractInstance(address);
      if (!instance) continue;
      const classId = lower(instance.originalContractClassId);
      classIds.add(classId);
      // oxlint-disable-next-line no-await-in-loop
      const published = await this.#context.node.node.getContract(address).catch(() => undefined);
      if (!published)
        instances.push({
          address: lower(address),
          instance: hex(new SerializableContractInstancePreimage(instance).toBuffer()),
        });
      // oxlint-disable-next-line no-await-in-loop
      const artifact = await pxe.getContractArtifact(instance.originalContractClassId);
      // oxlint-disable-next-line no-await-in-loop
      if (artifact) await this.#offer(classId, artifact).catch(() => undefined);
    }
    return { classIds: [...classIds], instances };
  }

  /**
   * Makes the contracts a proposal names usable here: classes this PXE lacks are fetched from the vault's members
   * (through the keeper) and checked; targets' instances are registered from the node or the payload. Returns whether
   * everything is here; a fetch in flight finishes with a keeper `artifact` event.
   */
  async ensure(vault: Hex32, targets: string[], contracts: PayloadContracts): Promise<boolean> {
    const pxe = this.#context.wallet().pxeService;
    let ready = true;
    for (const classId of contracts.classIds) {
      // oxlint-disable-next-line no-await-in-loop
      if (await pxe.getContractArtifact(Fr.fromString(classId))) continue;
      // oxlint-disable-next-line no-await-in-loop
      ready = (await this.fetched(vault, classId as Hex32)) && ready;
    }
    for (const target of new Set(targets)) {
      const address = AztecAddress.fromStringUnsafe(target);
      // oxlint-disable-next-line no-await-in-loop
      if (await pxe.getContractInstance(address)) continue;
      const given = contracts.instances.find((i) => i.address === lower(address));
      // oxlint-disable-next-line no-await-in-loop
      const instance = await (given
        ? this.#fromBytes(address, given.instance)
        : this.#published(address));
      if (!instance) {
        ready = false;
        continue;
      }
      // oxlint-disable-next-line no-await-in-loop
      const artifact = await pxe.getContractArtifact(instance.originalContractClassId);
      // oxlint-disable-next-line no-await-in-loop
      if (artifact) await this.#context.wallet().registerContract(instance, artifact);
      else ready = false;
    }
    return ready;
  }

  /**
   * Registers the class `classId` once the keeper holds it, after checking it; asks the keeper to fetch it otherwise.
   * Returns whether the class is now usable.
   */
  async fetched(vault: Hex32, classId: Hex32): Promise<boolean> {
    const keeper = this.#context.keeper.client;
    const status = await keeper.call("artifacts.fetch", { vault, classId });
    if (!status.complete || status.count === null) return false;
    const chunks = await Promise.all(
      Array.from({ length: status.count }, async (_, index) => {
        const chunk = await keeper.call("artifacts.get", { classId, index });
        if (!chunk) throw notReady("payload", "the artifact's chunks are not all here yet");
        return Buffer.from(chunk.bytes.slice(2), "hex");
      }),
    );
    const artifact = await unpack(chunks, classId);
    await this.#context.wallet().registerContractClass(artifact);
    await this.#save(classId, artifact);
    return true;
  }

  /** Puts an artifact in this member's keeper, chunk by chunk, unless it is already complete there. */
  async #offer(classId: Hex32, artifact: ContractArtifact): Promise<void> {
    const keeper = this.#context.keeper.client;
    const vault = this.#context.vaults.list()[0]?.address ?? classId;
    if ((await keeper.call("artifacts.status", { vault, classId })).complete) return;
    const chunks = pack(artifact);
    for (const [index, chunk] of chunks.entries()) {
      // oxlint-disable-next-line no-await-in-loop
      await keeper.call("artifacts.put", {
        classId,
        index,
        count: chunks.length,
        bytes: hex(chunk),
      });
    }
  }

  async #save(classId: Hex32, artifact: ContractArtifact): Promise<void> {
    await privateDir(this.#context.paths.contracts);
    await writePrivate(
      join(this.#context.paths.contracts, `${classId}.gz`),
      gzipSync(contractArtifactToBuffer(artifact)),
    );
  }

  async #saved(classId: Hex32): Promise<ContractArtifact | undefined> {
    const bytes = await readFile(join(this.#context.paths.contracts, `${classId}.gz`)).catch(
      () => undefined,
    );
    return bytes ? contractArtifactFromBuffer(gunzipSync(bytes)) : undefined;
  }

  /**
   * The instance at `address`, from saved bytes or published on the node. It must run `classId`: the class it was
   * deployed with, or, for an upgraded contract the node knows, the class it runs now.
   */
  async #instance(
    address: Hex32,
    classId: Hex32,
    saved: HexBytes | null,
  ): Promise<ContractInstancePreimageWithAddress> {
    const target = AztecAddress.fromStringUnsafe(address);
    if (saved) {
      const instance = await this.#fromBytes(target, saved);
      if (instance && lower(instance.originalContractClassId) === classId) return instance;
      throw new DaemonError(
        "invalid",
        `the saved instance of ${address} doesn't run class ${classId}`,
      );
    }
    const published = await this.#context.node.node.getContract(target).catch(() => undefined);
    if (!published)
      throw new DaemonError(
        "invalid",
        `the node has no published contract at ${address}; pass its deployment (salt, deployer, initialization hash, immutables hash, public keys)`,
      );
    const runs = [published.currentContractClassId, published.originalContractClassId].map(lower);
    if (!runs.includes(classId))
      throw new DaemonError(
        "invalid",
        `this artifact isn't the code of ${address}: it is class ${classId}, the contract runs ${runs[0]}`,
      );
    return published;
  }

  async #givenInstance(
    address: Hex32,
    classId: Hex32,
    input: NonNullable<ContractRegisterParams["instance"]>,
  ): Promise<ContractInstancePreimageWithAddress> {
    const preimage = {
      version: 2 as const,
      salt: Fr.fromString(input.salt),
      deployer: AztecAddress.fromStringUnsafe(input.deployer),
      originalContractClassId: Fr.fromString(classId),
      initializationHash: Fr.fromString(input.initializationHash),
      immutablesHash: Fr.fromString(input.immutablesHash),
      publicKeys: PublicKeys.fromString(input.publicKeys),
    };
    const derived = lower(await computeContractAddressFromInstance(preimage));
    if (derived !== address)
      throw new DaemonError("invalid", `that deployment derives ${derived}, not ${address}`);
    return { ...preimage, address: AztecAddress.fromStringUnsafe(address) };
  }

  async #published(
    address: AztecAddress,
  ): Promise<ContractInstancePreimageWithAddress | undefined> {
    return this.#context.node.node.getContract(address).catch(() => undefined);
  }

  /** An instance from its bytes, refused unless it derives `address`. */
  async #fromBytes(
    address: AztecAddress,
    bytes: HexBytes,
  ): Promise<ContractInstancePreimageWithAddress | undefined> {
    const instance = SerializableContractInstancePreimage.fromBuffer(
      Buffer.from(bytes.slice(2), "hex"),
    );
    const derived = await computeContractAddressFromInstance(instance);
    return derived.equals(address) ? instance.withAddress(address) : undefined;
  }
}
