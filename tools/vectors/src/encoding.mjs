// How values are written into vector files: fields and scalars as 0x-prefixed hex, points with every encoding the
// Rust side must reproduce.

/** A field element or scalar as Aztec prints it (0x-prefixed, 64 hex digits). */
export const hex = (value) => value.toString();

/** A Grumpkin point: coordinates plus its uncompressed and compressed byte encodings. */
export const point = (p) => ({
  x: hex(p.x),
  y: hex(p.y),
  bytes: `0x${p.toBuffer().toString("hex")}`,
  compressed: `0x${p.toCompressedBuffer().toString("hex")}`,
});
