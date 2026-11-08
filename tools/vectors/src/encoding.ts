// How values are written into vector files: fields and scalars as 0x-prefixed hex, points with every encoding the
// Rust side must reproduce.

import type { Point } from "@aztec-labs/foundation/curves/grumpkin";

/** A point's coordinates and its uncompressed and compressed byte encodings. */
export interface EncodedPoint {
  x: string;
  y: string;
  bytes: string;
  compressed: string;
}

/** A field element or scalar as Aztec prints it (0x-prefixed, 64 hex digits). */
export const hex = (value: { toString(): string }): string => value.toString();

export const point = (p: Point): EncodedPoint => ({
  x: hex(p.x),
  y: hex(p.y),
  bytes: `0x${p.toBuffer().toString("hex")}`,
  compressed: `0x${p.toCompressedBuffer().toString("hex")}`,
});
