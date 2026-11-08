//! Byte values as `0x`-prefixed lowercase hex, the convention of every Ome API. Parsing accepts either case and an
//! optional prefix.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use ts_rs::TS;

/// A hex string that does not decode to the expected bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HexError {
    expected: Option<usize>,
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.expected {
            Some(len) => write!(f, "expected {len} bytes of hex"),
            None => f.write_str("expected hex"),
        }
    }
}

impl std::error::Error for HexError {}

fn digits(text: &str) -> &str {
    text.strip_prefix("0x").unwrap_or(text)
}

fn prefixed(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

macro_rules! fixed_hex {
    ($(#[$doc:meta])* $name:ident, $len:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, TS)]
        #[ts(type = "string")]
        pub struct $name(pub [u8; $len]);

        impl FromStr for $name {
            type Err = HexError;

            fn from_str(text: &str) -> Result<Self, HexError> {
                let mut out = [0u8; $len];
                hex::decode_to_slice(digits(text), &mut out).map_err(|_| HexError { expected: Some($len) })?;
                Ok(Self(out))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&prefixed(&self.0))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }

        impl From<[u8; $len]> for $name {
            fn from(bytes: [u8; $len]) -> Self {
                Self(bytes)
            }
        }

        impl From<$name> for [u8; $len] {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                text.parse().map_err(de::Error::custom)
            }
        }
    };
}

fixed_hex!(
    /// 32 bytes: addresses, field elements, hashes, keys, session ids.
    Hex32,
    32
);
fixed_hex!(
    /// 64 bytes: ceremony transcript hashes.
    Hex64,
    64
);
fixed_hex!(
    /// 16 bytes: lobby ids.
    Hex16,
    16
);

/// Bytes of any length: canonical wire encodings of proposals, payloads and votes.
#[derive(Clone, PartialEq, Eq, Hash, Default, TS)]
#[ts(type = "string")]
pub struct HexBytes(pub Vec<u8>);

impl FromStr for HexBytes {
    type Err = HexError;

    fn from_str(text: &str) -> Result<Self, HexError> {
        hex::decode(digits(text))
            .map(Self)
            .map_err(|_| HexError { expected: None })
    }
}

impl fmt::Display for HexBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&prefixed(&self.0))
    }
}

impl fmt::Debug for HexBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl From<Vec<u8>> for HexBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl Serialize for HexBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for HexBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

/// A Grumpkin point as its affine coordinates. Whether it is on the curve is checked where it is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Point {
    /// `x`.
    pub x: Hex32,
    /// `y`.
    pub y: Hex32,
}
