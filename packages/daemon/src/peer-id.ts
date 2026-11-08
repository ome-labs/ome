// libp2p peer ids of Ed25519 keys, as keepers advertise them: base58btc of the identity multihash of the protobuf
// public key (`0x00 0x24` multihash prefix, `0x08 0x01 0x12 0x20` key type and length).

const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const PREFIX = [0x00, 0x24, 0x08, 0x01, 0x12, 0x20];

function base58(bytes: Uint8Array): string {
  let value = BigInt(`0x${Buffer.from(bytes).toString("hex")}`);
  let out = "";
  while (value > 0n) {
    out = ALPHABET[Number(value % 58n)] + out;
    value /= 58n;
  }
  for (const byte of bytes) {
    if (byte !== 0) break;
    out = `1${out}`;
  }
  return out;
}

/** The peer id of a keeper identity (`0x` hex of its Ed25519 public key). */
export function peerId(identity: string): string {
  const key = Buffer.from(identity.replace(/^0x/u, ""), "hex");
  return base58(Uint8Array.from([...PREFIX, ...key]));
}
