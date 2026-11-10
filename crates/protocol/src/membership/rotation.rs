use ome_crypto::{Base, Error as CryptoError, Point, threshold::shamir::Commitments};
use ome_wire::types::{field, parse_field};

use super::Vault;
use crate::{
    Error,
    payload::{Call, CallKind},
    vault::{
        Committee, Config, SignerSet,
        constants::{
            MAX_SIGNERS, POINT_LEN, ROTATE_MEMBERSHIP_ARGS, ROTATE_MEMBERSHIP_SELECTOR,
            ROTATE_MEMBERSHIP_SIGNATURE,
        },
        padded,
    },
};

/// Fields of one padded point list in the call's arguments.
const KEYS: usize = MAX_SIGNERS * POINT_LEN;

/// A proposed `rotate_membership`: the new signer set, spending threshold and committee from the reshare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rotation {
    /// New signer keys in slot order.
    pub signers: SignerSet,
    /// New spending threshold `k`.
    pub threshold: u32,
    /// New viewing committee: `t_v` and the verification shares the reshare produced.
    pub committee: Committee,
}

impl Rotation {
    /// The `rotate_membership` call that performs this rotation on `vault`.
    #[must_use]
    pub fn to_call(&self, vault: &Vault) -> Call {
        let mut args = Vec::with_capacity(ROTATE_MEMBERSHIP_ARGS);
        args.extend_from_slice(&padded(vault.signers().keys()));
        args.extend_from_slice(&padded(self.signers.keys()));
        args.extend(
            [
                self.signers.count(),
                self.threshold,
                self.committee.viewing_threshold(),
            ]
            .map(Base::from),
        );
        args.extend_from_slice(&padded(self.committee.shares()));
        args.extend_from_slice(&padded(&polynomial(&self.committee)));
        Call {
            signature: Some(ROTATE_MEMBERSHIP_SIGNATURE.to_owned()),
            to: field(&vault.address()),
            selector: ROTATE_MEMBERSHIP_SELECTOR,
            kind: CallKind::Private,
            is_static: false,
            hide_msg_sender: false,
            args: args.iter().map(field).collect(),
        }
    }

    /// Decodes a `rotate_membership` call on `vault`: current keys, new keys, new count, `k`, `t_v`, the committee
    /// shares, then the committee polynomial, each point list zero-padded to eight. The polynomial must be the one the
    /// shares determine ([`polynomial`]), so an approved call is exactly this rotation.
    ///
    /// # Errors
    /// [`Error::InvalidRotationCall`] if the call is not `rotate_membership` on this vault or its arguments are not a
    /// well-formed rotation of the current signer set, or signer-set and committee errors.
    pub fn from_call(call: &Call, vault: &Vault) -> Result<Self, Error> {
        if !is_rotation_call(call, vault) {
            return Err(Error::InvalidRotationCall(
                "not rotate_membership on this vault",
            ));
        }
        Self::decode(call, vault.signers().keys())
    }

    /// Decodes a `rotate_membership` call's arguments, requiring its current keys to be `current`.
    pub(super) fn decode(call: &Call, current: &[Point]) -> Result<Self, Error> {
        if call.kind != CallKind::Private {
            return Err(Error::InvalidRotationCall(
                "not rotate_membership on this vault",
            ));
        }
        if call.args.len() != ROTATE_MEMBERSHIP_ARGS {
            return Err(Error::InvalidRotationCall("wrong argument count"));
        }
        let args = call
            .args
            .iter()
            .map(parse_field)
            .collect::<Result<Vec<_>, _>>()?;
        if args[..KEYS] != padded(current) {
            return Err(Error::InvalidRotationCall(
                "current keys are not the vault's signer set",
            ));
        }
        let count = small(args[2 * KEYS])?;
        let threshold = small(args[2 * KEYS + 1])?;
        let viewing_threshold = small(args[2 * KEYS + 2])?;
        let signers = SignerSet::new(points(&args[KEYS..2 * KEYS], count)?)?;
        let committee = Committee::new(
            viewing_threshold,
            points(&args[2 * KEYS + 3..3 * KEYS + 3], count)?,
        )?;
        let commitments = points(&args[3 * KEYS + 3..], viewing_threshold)?;
        if commitments != polynomial(&committee) {
            return Err(Error::InvalidRotationCall(
                "committee polynomial is not the shares' own",
            ));
        }
        Ok(Self {
            signers,
            threshold,
            committee,
        })
    }
}

/// The Feldman commitments `C_0 … C_{t_v - 1}` of the polynomial the committee's first `t_v` shares determine
/// (Lagrange interpolation in the exponent). The contract checks every share against them and `C_0` against the
/// vault's `Ivpk`. They derive from public verification shares and reveal nothing secret. Empty when the shares are
/// degenerate, which the contract refuses.
#[must_use]
pub fn polynomial(committee: &Committee) -> Vec<Point> {
    let threshold = usize::try_from(committee.viewing_threshold()).unwrap_or(usize::MAX);
    let evaluations = committee.evaluations();
    let basis = &evaluations[..threshold.min(evaluations.len())];
    Commitments::interpolate(threshold, basis)
        .map(|c| c.points().to_vec())
        .unwrap_or_default()
}

/// Whether `call` is a `rotate_membership` call on `vault`.
#[must_use]
pub fn is_rotation_call(call: &Call, vault: &Vault) -> bool {
    call.selector == ROTATE_MEMBERSHIP_SELECTOR
        && parse_field(&call.to).ok() == Some(vault.address())
}

/// The first `count` points of a zero-padded list; every later slot must be zero.
fn points(fields: &[Base], count: u32) -> Result<Vec<Point>, Error> {
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    if count > MAX_SIGNERS {
        return Err(Error::InvalidRotationCall("member count above eight"));
    }
    let (used, rest) = fields.split_at(count * POINT_LEN);
    if rest.iter().any(|f| *f != Base::from(0u64)) {
        return Err(Error::InvalidRotationCall(
            "non-empty slot past the member count",
        ));
    }
    used.chunks(POINT_LEN)
        .map(|xy| Ok(Point::from_xy(xy[0], xy[1])?))
        .collect()
}

/// A `u32` argument.
pub(super) fn small(value: Base) -> Result<u32, Error> {
    let bytes = field(&value);
    if bytes[..28].iter().any(|b| *b != 0) {
        return Err(Error::InvalidRotationCall("count or threshold above u32"));
    }
    Ok(u32::from_be_bytes([
        bytes[28], bytes[29], bytes[30], bytes[31],
    ]))
}

/// Checks that `committee`'s verification shares lie on one polynomial of degree `t_v - 1` whose value at zero is
/// `group_key`, so the committee can decrypt for the vault and nothing else.
///
/// # Errors
/// [`Error::ShareOffPolynomial`] naming the first share off the polynomial, or [`Error::GroupKeyMismatch`].
pub fn verify_shares(group_key: &Point, committee: &Committee) -> Result<(), Error> {
    let threshold = usize::try_from(committee.viewing_threshold()).unwrap_or(usize::MAX);
    let commitments = Commitments::interpolate(threshold, &committee.evaluations()).map_err(
        |error| match error {
            CryptoError::InvalidShare(index) => Error::ShareOffPolynomial(index),
            other => Error::Crypto(other),
        },
    )?;
    if commitments.constant_term() != group_key {
        return Err(Error::GroupKeyMismatch);
    }
    Ok(())
}

/// Everything an approver checks before signing a rotation, returning the config it produces:
///
/// - the contract's own rules: the current set matches, at most `t_v - 1` members leave, thresholds in range;
/// - the new shares belong to the vault's viewing key (the contract checks only that they are well formed);
/// - the committee was reshared: no new share equals a current one, so removed members' shares stop working.
///
/// A rotation that fails is refused, not warned about: a bad committee can leave the vault unreadable for good.
///
/// # Errors
/// Config transition errors, [`Error::ShareOffPolynomial`], [`Error::GroupKeyMismatch`] or
/// [`Error::SharesNotRefreshed`].
pub fn check_rotation(vault: &Vault, rotation: &Rotation) -> Result<Config, Error> {
    let next = vault.config().rotate(
        vault.signers(),
        &rotation.signers,
        rotation.threshold,
        &rotation.committee,
    )?;
    verify_shares(vault.group_key(), &rotation.committee)?;
    if rotation
        .committee
        .shares()
        .iter()
        .any(|share| vault.committee().shares().contains(share))
    {
        return Err(Error::SharesNotRefreshed);
    }
    Ok(next)
}
