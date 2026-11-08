// Aztec's field and point classes against the `@ome-labs/api` shapes the keeper and the bindings use: `0x` hex fields
// and `{ x, y }` points.
import { Fr } from "@aztec-labs/foundation/curves/bn254";
import { Point } from "@aztec-labs/foundation/curves/grumpkin";
import type { Hex32, Point as ApiPoint } from "@ome-labs/api";

export const toApiPoint = (point: Point): ApiPoint => ({
  x: point.x.toString(),
  y: point.y.toString(),
});

export const fromApiPoint = (point: ApiPoint): Point =>
  new Point(Fr.fromString(point.x), Fr.fromString(point.y));

export const toHex32 = (field: Fr | { toField(): Fr }): Hex32 =>
  (field instanceof Fr ? field : field.toField()).toString();

export const fromHex32 = (hex: Hex32): Fr => Fr.fromString(hex);
