// Contract artifacts as files and keeper chunks: the gzip-compressed artifact, split under the wire's chunk limit,
// and the one check that makes an artifact from anyone usable: it computes to the class it claims.
import { gunzipSync, gzipSync } from "node:zlib";

import {
  type ContractArtifact,
  contractArtifactFromBuffer,
  contractArtifactToBuffer,
  loadContractArtifact,
} from "@aztec-labs/stdlib/abi";
import { getContractClassFromArtifact } from "@aztec-labs/stdlib/contract";
import type { Hex32 } from "@ome-labs/api";

import { DaemonError } from "../errors.ts";

/** The wire's chunk size (`MAX_ARTIFACT_CHUNK_BYTES`). */
export const CHUNK_BYTES = 256 * 1024;
/** Largest artifact, uncompressed. */
export const MAX_ARTIFACT_BYTES = 32 << 20;

/** The artifact's class id. */
export async function classIdOf(artifact: ContractArtifact): Promise<Hex32> {
  return (await getContractClassFromArtifact(artifact)).id.toString() as Hex32;
}

/**
 * An artifact a member gave as JSON text: the compiler's output (`aztec compile`), or an artifact Aztec.js wrote out.
 */
export function parseArtifact(text: string): ContractArtifact {
  if (Buffer.byteLength(text) > MAX_ARTIFACT_BYTES)
    throw new DaemonError("invalid", "the artifact is larger than 32 MiB");
  let json: { functions?: { abi?: unknown }[] };
  try {
    json = JSON.parse(text) as typeof json;
  } catch {
    throw new DaemonError("invalid", "the artifact is not JSON");
  }
  try {
    return json.functions?.[0]?.abi === undefined
      ? contractArtifactFromBuffer(Buffer.from(text, "utf8"))
      : loadContractArtifact(json as never);
  } catch (error) {
    throw new DaemonError(
      "invalid",
      `the artifact doesn't parse: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

/** The artifact gzip-compressed, in chunks of at most {@link CHUNK_BYTES}. */
export function pack(artifact: ContractArtifact): Buffer[] {
  const packed = gzipSync(contractArtifactToBuffer(artifact));
  const chunks: Buffer[] = [];
  for (let at = 0; at < packed.length; at += CHUNK_BYTES)
    chunks.push(packed.subarray(at, at + CHUNK_BYTES));
  return chunks;
}

/** Chunks back to an artifact, refused unless it computes to `classId`: whoever sent it isn't trusted for it. */
export async function unpack(chunks: Buffer[], classId: Hex32): Promise<ContractArtifact> {
  let artifact: ContractArtifact;
  try {
    const bytes = gunzipSync(Buffer.concat(chunks), { maxOutputLength: MAX_ARTIFACT_BYTES });
    artifact = contractArtifactFromBuffer(bytes);
  } catch {
    throw new DaemonError("invalid", `the artifact received for class ${classId} doesn't decode`);
  }
  const computed = await classIdOf(artifact);
  if (computed !== classId)
    throw new DaemonError(
      "invalid",
      `the artifact received for class ${classId} is the code of class ${computed}`,
    );
  return artifact;
}
