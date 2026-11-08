//! The vector files: `contract.json` (shared with the Noir tests) and `payload.json` (from aztec.js).

use ome_crypto::{Base, Point};
use ome_wire::types::parse_field;
use serde_json::Value;

pub fn contract() -> Value {
    serde_json::from_str(include_str!("../vectors/contract.json")).unwrap()
}

pub fn payload() -> Value {
    serde_json::from_str(include_str!("../vectors/payload.json")).unwrap()
}

pub fn base(value: &Value) -> Base {
    let text = value.as_str().unwrap().trim_start_matches("0x");
    let mut bytes = [0u8; 32];
    let digits = format!("{text:0>64}");
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).unwrap();
    }
    parse_field(&bytes).unwrap()
}

/// A point, or `None` for the zero padding slot.
pub fn point(value: &Value) -> Option<Point> {
    let (x, y) = (base(&value["x"]), base(&value["y"]));
    (x != Base::from(0u64) || y != Base::from(0u64)).then(|| Point::from_xy(x, y).unwrap())
}
