use ome_crypto::{Base, poseidon2};
use ome_wire::types::{WireField, parse_field};
use serde::{Deserialize, Serialize};

use super::selector;
use crate::{
    Error,
    vault::constants::{CALL_FIELDS, DOM_SEP_FUNCTION_ARGS, DOM_SEP_PUBLIC_CALLDATA},
};

/// Whether a call runs in private or is enqueued for public execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallKind {
    /// A private function call.
    Private,
    /// An enqueued public function call.
    Public,
}

/// One call of a vault transaction, as Aztec's `FunctionCall` describes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    /// The function signature, e.g. `transfer_private_to_private((Field),(Field),u128,Field)`, when known. It must hash
    /// to `selector`, so a summary built from it shows the function really called.
    pub signature: Option<String>,
    /// Target contract address.
    pub to: WireField,
    /// Function selector.
    pub selector: u32,
    /// Private or public.
    pub kind: CallKind,
    /// Whether the call may not modify state.
    pub is_static: bool,
    /// Hide the vault as `msg_sender` (public calls only).
    pub hide_msg_sender: bool,
    /// Encoded arguments.
    pub args: Vec<WireField>,
}

impl Call {
    /// The call's checks: a non-zero target (the entrypoint silently skips zero targets), canonical fields, no hidden
    /// sender on a private call, and a signature that matches the selector.
    ///
    /// # Errors
    /// [`Error::InvalidCall`] naming the call and the reason.
    pub fn validate(&self, index: usize) -> Result<(), Error> {
        let invalid = |reason| Error::InvalidCall { index, reason };
        let to = parse_field(&self.to).map_err(|_| invalid("target is not a field"))?;
        if to == Base::from(0u64) {
            return Err(invalid("zero target"));
        }
        if self.kind == CallKind::Private && self.hide_msg_sender {
            return Err(invalid("hide_msg_sender on a private call"));
        }
        if self.args.iter().any(|arg| parse_field(arg).is_err()) {
            return Err(invalid("argument is not a field"));
        }
        if let Some(signature) = &self.signature
            && selector(signature).ok() != Some(self.selector)
        {
            return Err(invalid("signature does not match the selector"));
        }
        Ok(())
    }

    /// The args hash the entrypoint commits to: `computeVarArgsHash(args)` for a private call (zero without
    /// arguments), `computeCalldataHash([selector, ...args])` for a public one.
    ///
    /// # Errors
    /// [`Error::Wire`] for a non-canonical argument.
    pub fn args_hash(&self) -> Result<Base, Error> {
        let args = self
            .args
            .iter()
            .map(parse_field)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(match self.kind {
            CallKind::Private if args.is_empty() => Base::from(0u64),
            CallKind::Private => {
                poseidon2::hash_with_separator(Base::from(DOM_SEP_FUNCTION_ARGS), &args)
            }
            CallKind::Public => calldata_hash(self.selector, &args),
        })
    }

    /// `[args_hash, selector, target, is_public, hide_msg_sender, is_static]`.
    pub(super) fn fields(&self) -> Result<[Base; CALL_FIELDS], Error> {
        Ok([
            self.args_hash()?,
            Base::from(self.selector),
            parse_field(&self.to)?,
            flag(self.kind == CallKind::Public),
            flag(self.hide_msg_sender),
            flag(self.is_static),
        ])
    }
}

/// The fields of a padding slot: Aztec pads with `FunctionCall.empty()`, a public call with selector 0, target 0 and
/// no arguments, so its args hash is the calldata hash of `[0]`, not zero.
pub(super) fn empty_call_fields() -> [Base; CALL_FIELDS] {
    let zero = Base::from(0u64);
    [calldata_hash(0, &[]), zero, zero, flag(true), zero, zero]
}

fn calldata_hash(selector: u32, args: &[Base]) -> Base {
    let mut calldata = Vec::with_capacity(1 + args.len());
    calldata.push(Base::from(selector));
    calldata.extend_from_slice(args);
    poseidon2::hash_with_separator(Base::from(DOM_SEP_PUBLIC_CALLDATA), &calldata)
}

fn flag(value: bool) -> Base {
    Base::from(u64::from(value))
}
