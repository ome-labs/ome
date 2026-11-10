use aztec::macros::aztec;

/// Test-only router for the authwit end-to-end tests. Never deploy it outside tests.
///
/// `route` asks another instance to `pull` tokens from its own caller, so the contract the token sees as caller (the
/// authwit's caller) is one the transaction reaches only through the router.
#[aztec]
pub contract TestRouter {
    use aztec::{
        macros::functions::external,
        protocol::{
            abis::function_selector::FunctionSelector, address::AztecAddress, traits::ToField,
        },
    };

    #[external("private")]
    fn route(puller: AztecAddress, token: AztecAddress, amount: u128, authwit_nonce: Field) {
        let from = self.msg_sender();
        let selector =
            comptime { FunctionSelector::from_signature("pull((Field),(Field),u128,Field)") };
        let _ = self.context.call_private_function(
            puller,
            selector,
            [token.to_field(), from.to_field(), amount as Field, authwit_nonce],
        );
    }

    #[external("private")]
    fn pull(token: AztecAddress, from: AztecAddress, amount: u128, authwit_nonce: Field) {
        let selector = comptime {
            FunctionSelector::from_signature("transfer_to_public((Field),(Field),u128,Field)")
        };
        let _ = self.context.call_private_function(
            token,
            selector,
            [from.to_field(), self.address.to_field(), amount as Field, authwit_nonce],
        );
    }
}
