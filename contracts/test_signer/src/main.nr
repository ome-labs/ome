use aztec::macros::aztec;

/// Test-only helper that hands back Schnorr signatures produced by the TXE. Never deploy it outside tests.
///
/// `TestEnvironment` can sign a message with a test account's key (`add_authwit`) but only exposes the signature to
/// private execution. Calling `signature` privately returns its limbs so vault tests can assemble multi-signer
/// witnesses.
#[aztec]
pub contract TestSigner {
    use aztec::{macros::functions::external, oracle::auth_witness::get_auth_witness};

    #[external("private")]
    fn signature(message: Field) -> [Field; 4] {
        // Safety: test-only helper; the returned limbs are verified by the contract under test.
        unsafe {
            get_auth_witness(message)
        }
    }
}
