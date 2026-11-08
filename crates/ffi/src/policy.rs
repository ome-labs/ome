//! The member's own rules on this phone, checked before it signs: which contracts a proposal may call, how much of a
//! token one proposal may move, and whether every contract must carry a label pinned on the phone.

use ome_protocol::payload::{Call, arguments};
use ome_wire::types::WireField;
use serde::{Deserialize, Serialize};

use crate::{FfiError, error::Result};

/// Local spending rules. The default allows everything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    /// Contracts a proposal may call (hex addresses); empty allows every contract. Trusted fee payers are always
    /// allowed.
    pub allowed_contracts: Vec<String>,
    /// Most of a token one proposal may move.
    pub token_limits: Vec<TokenLimit>,
    /// Refuse any call to a contract without a label pinned below.
    pub require_pinned_labels: bool,
    /// Names this phone trusts for addresses; the daemon's labels show as claimed.
    pub pins: Vec<Pin>,
}

/// The most of `token` one proposal may move: the first `u128` argument of each call to the token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenLimit {
    /// Token contract (hex).
    pub token: String,
    /// Largest amount in base units, as a decimal string.
    pub max_amount: String,
}

/// A name pinned on this phone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    /// Address (hex).
    pub address: String,
    /// Name.
    pub label: String,
}

impl Policy {
    /// Checks that every address and amount parses.
    pub(crate) fn validate(&self) -> Result<()> {
        for address in &self.allowed_contracts {
            parse_address(address)?;
        }
        for limit in &self.token_limits {
            parse_address(&limit.token)?;
            parse_amount(&limit.max_amount)?;
        }
        for pin in &self.pins {
            parse_address(&pin.address)?;
            if pin.label.is_empty() || pin.label.len() > ome_wire::limits::MAX_NAME_BYTES {
                return Err(FfiError::invalid("a pinned label is 1 to 64 bytes"));
            }
        }
        Ok(())
    }

    /// The label this phone pinned for `address`.
    pub(crate) fn pinned(&self, address: &WireField) -> Option<&str> {
        self.pins
            .iter()
            .find(|p| parse_address(&p.address).ok().as_ref() == Some(address))
            .map(|p| p.label.as_str())
    }

    /// What in `calls` this policy refuses; calls to a trusted fee payer are exempt.
    pub(crate) fn violations(&self, calls: &[Call], fee_payers: &[WireField]) -> Vec<String> {
        let allowed: Vec<WireField> = self
            .allowed_contracts
            .iter()
            .filter_map(|a| parse_address(a).ok())
            .collect();
        let mut moved: Vec<(WireField, u128)> = Vec::new();
        let mut out = Vec::new();
        for call in calls.iter().filter(|c| !fee_payers.contains(&c.to)) {
            let to = display_address(&call.to);
            if !allowed.is_empty() && !allowed.contains(&call.to) {
                out.push(format!("calls {to}, which this phone does not allow"));
            }
            if self.require_pinned_labels && self.pinned(&call.to).is_none() {
                out.push(format!(
                    "calls {to}, which has no label pinned on this phone"
                ));
            }
            if let Some(amount) = first_amount(call) {
                match moved.iter_mut().find(|(token, _)| *token == call.to) {
                    Some((_, total)) => *total = total.saturating_add(amount),
                    None => moved.push((call.to, amount)),
                }
            }
        }
        for limit in &self.token_limits {
            let (Ok(token), Ok(max)) =
                (parse_address(&limit.token), parse_amount(&limit.max_amount))
            else {
                continue;
            };
            if let Some((_, total)) = moved.iter().find(|(t, _)| *t == token)
                && *total > max
            {
                out.push(format!(
                    "moves {total} of {}, above this phone's limit of {max}",
                    display_address(&token)
                ));
            }
        }
        out
    }
}

/// The first `u128` argument of a call with a known signature.
fn first_amount(call: &Call) -> Option<u128> {
    let signature = call.signature.as_deref()?;
    arguments(signature, &call.args)
        .ok()?
        .into_iter()
        .find(|a| a.kind == "u128")
        .and_then(|a| a.fields.first().and_then(field_u128))
}

/// A field as a `u128`, if it fits.
pub(crate) fn field_u128(field: &WireField) -> Option<u128> {
    let (high, low) = field.split_at(16);
    if high.iter().any(|b| *b != 0) {
        return None;
    }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(low);
    Some(u128::from_be_bytes(bytes))
}

/// A hex address, `0x` optional, left-padded to 32 bytes.
pub(crate) fn parse_address(text: &str) -> Result<WireField> {
    let digits = text.trim().strip_prefix("0x").unwrap_or(text.trim());
    if digits.is_empty() || digits.len() > 64 {
        return Err(FfiError::invalid(format!("{text} is not an address")));
    }
    let padded = format!("{digits:0>64}");
    let mut out = [0u8; 32];
    hex::decode_to_slice(padded, &mut out)
        .map_err(|_| FfiError::invalid(format!("{text} is not an address")))?;
    Ok(out)
}

fn parse_amount(text: &str) -> Result<u128> {
    text.parse()
        .map_err(|_| FfiError::invalid(format!("{text} is not a whole amount")))
}

pub(crate) fn display_address(address: &WireField) -> String {
    format!("0x{}", hex::encode(address))
}
