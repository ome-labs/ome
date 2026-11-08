//! Complete ceremonies run in one process: key generation, resharing, dealing a known secret, and threshold ECDH.

use ome_crypto::{
    Base, Point, SecretScalar,
    aztec::AddressContext,
    threshold::{
        dkg::{self, KeyShare},
        ecdh::{AddressSecretShare, EcdhContext, Partial, combine},
        reshare::{self, SubShare},
        shamir::{MemberIndex, Polynomial},
    },
};
use rand_chacha::ChaCha20Rng;
use rand_core::{Rng, SeedableRng};

/// DKG parameters for members `1..=n` with threshold `t`.
pub fn dkg_parameters(t: usize, n: u32) -> dkg::Parameters {
    dkg::Parameters {
        session: [7u8; 32],
        threshold: t,
        participants: (1..=n).collect(),
    }
}

/// Every member's round-1 secret, the broadcast packages, and every private round-2 package.
pub fn dkg_rounds(
    parameters: &dkg::Parameters,
    rng: &mut ChaCha20Rng,
) -> (
    Vec<dkg::Round1Secret>,
    Vec<dkg::Round1Package>,
    Vec<dkg::Round2Package>,
) {
    let (secrets, round1): (Vec<_>, Vec<_>) = parameters
        .participants
        .iter()
        .map(|&i| dkg::round1(parameters, i, rng).unwrap())
        .unzip();

    let round2 = secrets
        .iter()
        .flat_map(|secret| dkg::round2(parameters, secret, &round1).unwrap())
        .collect();

    (secrets, round1, round2)
}

/// A full honest DKG for members `1..=n` with threshold `t`.
pub fn run_dkg(t: usize, n: u32, rng: &mut ChaCha20Rng) -> Vec<KeyShare> {
    let parameters = dkg_parameters(t, n);
    let (secrets, round1, round2) = dkg_rounds(&parameters, rng);

    secrets
        .into_iter()
        .map(|secret| dkg::finalize(&parameters, secret, &round1, &round2).unwrap())
        .collect()
}

/// Reshare parameters: `dealers` of a threshold-`old_t` committee deal to a new committee `1..=new_n`.
pub fn reshare_parameters(
    dealers: Vec<MemberIndex>,
    old_t: usize,
    new_n: u32,
    new_t: usize,
) -> reshare::Parameters {
    reshare::Parameters {
        session: [9u8; 32],
        dealers,
        old_threshold: old_t,
        new_participants: (1..=new_n).collect(),
        new_threshold: new_t,
    }
}

/// A full honest reshare from `old` to the new committee in `parameters`.
pub fn run_reshare(
    old: &[KeyShare],
    parameters: &reshare::Parameters,
    rng: &mut ChaCha20Rng,
) -> Vec<KeyShare> {
    let dealt: Vec<_> = parameters
        .dealers
        .iter()
        .map(|dealer| {
            let current = old.iter().find(|share| share.index() == *dealer).unwrap();
            reshare::deal(parameters, current, rng).unwrap()
        })
        .collect();
    let packages: Vec<_> = dealt.iter().map(|(package, _)| package.clone()).collect();

    parameters
        .new_participants
        .iter()
        .map(|&recipient| {
            let received = sub_shares_for(recipient, dealt.iter().flat_map(|(_, subs)| subs));
            reshare::receive(
                parameters,
                old[0].group_commitments(),
                recipient,
                &packages,
                &received,
            )
            .unwrap()
        })
        .collect()
}

/// The sub-shares addressed to `recipient`, as that member would receive them over the wire.
pub fn sub_shares_for<'a>(
    recipient: MemberIndex,
    all: impl IntoIterator<Item = &'a SubShare>,
) -> Vec<SubShare> {
    all.into_iter()
        .filter(|sub| sub.recipient == recipient)
        .map(|sub| SubShare {
            dealer: sub.dealer,
            recipient,
            share: SecretScalar::new(*sub.share.expose()),
        })
        .collect()
}

/// Deals a known `ivsk` into `t`-of-`n` shares of the address secret, as a DKG would have produced them.
pub fn deal_address_shares(
    ivsk: &SecretScalar,
    preaddress: Base,
    t: usize,
    n: u32,
    rng: &mut ChaCha20Rng,
) -> (Vec<AddressSecretShare>, AddressContext) {
    let ivpk = Point::generator_mul_secret(ivsk).unwrap();
    let context = AddressContext::new(preaddress, &ivpk).unwrap();
    let polynomial = Polynomial::random(SecretScalar::new(*ivsk.expose()), t, rng).unwrap();

    let shares = (1..=n)
        .map(|i| AddressSecretShare::new(i, &polynomial.evaluate(i).unwrap(), &context).unwrap())
        .collect();

    (shares, context)
}

/// The vault and epoch a test's partials are bound to.
pub fn ecdh_context(context: &AddressContext) -> EcdhContext {
    EcdhContext {
        vault: context.address(),
        epoch: 0,
    }
}

/// Threshold ECDH on `input` by exactly the given members.
pub fn threshold_ecdh(
    shares: &[&KeyShare],
    context: &AddressContext,
    input: &Point,
    rng: &mut ChaCha20Rng,
) -> Point {
    let partials: Vec<Partial> = shares
        .iter()
        .map(|share| {
            AddressSecretShare::new(share.index(), share.share(), context)
                .unwrap()
                .partial(input, &ecdh_context(context), rng)
                .unwrap()
        })
        .collect();

    combine(&partials, shares.len()).unwrap()
}

/// A random point with an unknown discrete log, standing in for a note's ephemeral key.
pub fn random_point(rng: &mut ChaCha20Rng) -> Point {
    Point::generator_mul_secret(&SecretScalar::random(rng)).unwrap()
}

/// Every `t`-member subset of `1..=n`.
pub fn subsets(n: u32, t: usize) -> Vec<Vec<MemberIndex>> {
    (0u32..(1 << n))
        .filter(|mask| mask.count_ones() as usize == t)
        .map(|mask| (1..=n).filter(|i| mask & (1 << (i - 1)) != 0).collect())
        .collect()
}

/// `k` distinct members of `1..=n`, chosen by `seed`.
pub fn random_subset(n: u32, k: usize, seed: u64) -> Vec<MemberIndex> {
    let mut all: Vec<MemberIndex> = (1..=n).collect();
    let mut rng = ChaCha20Rng::seed_from_u64(seed);

    for i in (1..all.len()).rev() {
        let j = usize::try_from(rng.next_u64() % (i as u64 + 1)).unwrap();
        all.swap(i, j);
    }

    all.truncate(k);
    all
}
