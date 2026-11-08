//! The auth witness has exactly the layout `ApprovalWitness::from_fields` decodes.
// Helpers outside `#[test]` unwrap like the tests do.
#![allow(clippy::unwrap_used)]

mod support;

use ome_crypto::{Base, schnorr};
use ome_protocol::vault::{
    Witness,
    constants::{MAX_SIGNERS, POINT_LEN, SIGNATURE_LEN, WITNESS_LEN},
};
use support::fixture::fixture;

#[test]
fn keys_signatures_and_deadline_sit_where_the_contract_reads_them() {
    let mut f = fixture();
    let signature = schnorr::sign(&f.signing_keys[2], Base::from(1u64), &mut f.rng).unwrap();
    let signers = f.vault.signers();

    let Witness(fields) = Witness::encode(signers, &[None, None, Some(signature)], 1_700_003_600);

    assert_eq!(WITNESS_LEN, 49);
    for (slot, key) in signers.keys().iter().enumerate() {
        assert_eq!(fields[slot * POINT_LEN], key.x());
        assert_eq!(fields[slot * POINT_LEN + 1], key.y());
    }
    let zero = Base::from(0u64);
    assert!(
        fields[signers.keys().len() * POINT_LEN..MAX_SIGNERS * POINT_LEN]
            .iter()
            .all(|f| *f == zero)
    );
    let signatures = MAX_SIGNERS * POINT_LEN;
    assert!(
        fields[signatures..signatures + 2 * SIGNATURE_LEN]
            .iter()
            .all(|f| *f == zero)
    );
    let third = signatures + 2 * SIGNATURE_LEN;
    assert_eq!(fields[third..third + SIGNATURE_LEN], signature.to_limbs());
    assert!(
        fields[third + SIGNATURE_LEN..WITNESS_LEN - 1]
            .iter()
            .all(|f| *f == zero)
    );
    assert_eq!(fields[WITNESS_LEN - 1], Base::from(1_700_003_600u64));
}

#[test]
fn signatures_past_the_signer_count_are_dropped() {
    let mut f = fixture();
    let signature = schnorr::sign(&f.signing_keys[0], Base::from(1u64), &mut f.rng).unwrap();

    let with_extra = Witness::encode(f.vault.signers(), &[None, None, None, Some(signature)], 9);

    assert_eq!(with_extra, Witness::encode(f.vault.signers(), &[], 9));
}
