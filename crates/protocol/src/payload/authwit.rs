use ome_crypto::{Base, poseidon2};
use ome_wire::types::{WireField, parse_field};
use serde::{Deserialize, Serialize};

use super::{Call, CallKind, ChainInfo, outer_hash};
use crate::{
    Error,
    vault::constants::{DOM_SEP_AUTHWIT_INNER, DOM_SEP_FUNCTION_ARGS},
};

/// A private authwit the vault grants with a transaction: `caller` may make `call` on the vault's behalf, once.
///
/// The consumer is `call.to` (a token, say), which asks the vault through `verify_private_authwit` whether its caller
/// may act for it. Members sign the message hash beside the transaction's, so the authwit needs the same `k` approvals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Authwit {
    /// The contract allowed to make the call, e.g. an AMM pulling the vault's tokens.
    pub caller: WireField,
    /// The call it may make: a private call whose `to` is the consumer.
    pub call: Call,
}

impl Authwit {
    /// The authwit's checks: a non-zero caller, a valid private call (public authwits live in the auth registry, set
    /// by a call of the transaction itself).
    ///
    /// # Errors
    /// [`Error::InvalidAuthwit`] naming the authwit and the reason.
    pub fn validate(&self, index: usize) -> Result<(), Error> {
        let invalid = |reason| Error::InvalidAuthwit { index, reason };
        let caller = parse_field(&self.caller).map_err(|_| invalid("caller is not a field"))?;
        if caller == Base::from(0u64) {
            return Err(invalid("zero caller"));
        }
        if self.call.kind != CallKind::Private {
            return Err(invalid("a private authwit authorizes a private call"));
        }
        self.call
            .validate(index)
            .map_err(|_| invalid("invalid call"))
    }

    /// `computeInnerAuthWitHashFromAction(caller, call)`: `Poseidon2(AUTHWIT_INNER, [caller, selector, args_hash])`
    /// with `computeVarArgsHash(args)` (zero without arguments).
    ///
    /// # Errors
    /// [`Error::Wire`] for a non-canonical field.
    pub fn inner_hash(&self) -> Result<Base, Error> {
        let args = self
            .call
            .args
            .iter()
            .map(parse_field)
            .collect::<Result<Vec<_>, _>>()?;
        let args_hash = if args.is_empty() {
            Base::from(0u64)
        } else {
            poseidon2::hash_with_separator(Base::from(DOM_SEP_FUNCTION_ARGS), &args)
        };
        Ok(poseidon2::hash_with_separator(
            Base::from(DOM_SEP_AUTHWIT_INNER),
            &[
                parse_field(&self.caller)?,
                Base::from(self.call.selector),
                args_hash,
            ],
        ))
    }

    /// The message hash the vault approves: the outer hash of the inner hash, consumed by `call.to` on `chain`.
    ///
    /// # Errors
    /// [`Error::Wire`] for a non-canonical field.
    pub fn message_hash(&self, chain: ChainInfo) -> Result<Base, Error> {
        Ok(outer_hash(
            parse_field(&self.call.to)?,
            chain,
            self.inner_hash()?,
        ))
    }
}
