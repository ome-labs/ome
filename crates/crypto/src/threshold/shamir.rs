//! Shamir secret sharing over the Grumpkin scalar field with Feldman commitments.
//!
//! Member indices are `1..=n` (never zero: the secret sits at `x = 0`). A degree-`t - 1` polynomial is reconstructible
//! from any `t` evaluations; its Feldman commitments `C_k = a_k · G` let anyone check a share without learning it.

use ark_ec::AffineRepr;
use ark_ff::{Field, One, Zero};
use ark_grumpkin::Projective;
use rand_core::CryptoRng;

use crate::{Error, Point, Scalar, SecretScalar};

/// A member's position in a committee, `1..=n`.
pub type MemberIndex = u32;

/// A secret polynomial `f(x) = a_0 + a_1 x + ... + a_{t-1} x^{t-1}`.
pub struct Polynomial {
    coefficients: Vec<SecretScalar>,
}

impl Polynomial {
    /// Random polynomial of the given `threshold` (degree `threshold - 1`) with `f(0) = constant`.
    ///
    /// # Errors
    /// [`Error::InvalidThreshold`] for a zero threshold.
    pub fn random(
        constant: SecretScalar,
        threshold: usize,
        rng: &mut impl CryptoRng,
    ) -> Result<Self, Error> {
        if threshold == 0 {
            return Err(Error::InvalidThreshold);
        }
        let mut coefficients = Vec::with_capacity(threshold);
        coefficients.push(constant);
        for _ in 1..threshold {
            coefficients.push(SecretScalar::random(rng));
        }
        Ok(Self { coefficients })
    }

    /// `f(0)`.
    #[must_use]
    pub fn constant(&self) -> &SecretScalar {
        &self.coefficients[0]
    }

    /// Number of evaluations needed to reconstruct `f(0)`.
    #[must_use]
    pub fn threshold(&self) -> usize {
        self.coefficients.len()
    }

    /// `f(index)` by Horner's rule.
    ///
    /// # Errors
    /// [`Error::InvalidIndex`] for index zero.
    pub fn evaluate(&self, index: MemberIndex) -> Result<SecretScalar, Error> {
        if index == 0 {
            return Err(Error::InvalidIndex);
        }
        let x = Scalar::from(index);
        let mut acc = Scalar::zero();
        for coefficient in self.coefficients.iter().rev() {
            acc = acc * x + coefficient.expose();
        }
        Ok(SecretScalar::new(acc))
    }

    /// Feldman commitments `C_k = a_k · G` (constant-time multiplications).
    ///
    /// # Errors
    /// [`Error::IdentityPoint`] if a coefficient is zero (probability negligible for random coefficients).
    pub fn commitments(&self) -> Result<Commitments, Error> {
        let points = self
            .coefficients
            .iter()
            .map(Point::generator_mul_secret)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Commitments(points))
    }
}

/// Feldman commitments to a polynomial's coefficients, constant term first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commitments(Vec<Point>);

impl Commitments {
    /// Wraps received commitments.
    ///
    /// # Errors
    /// [`Error::InvalidThreshold`] if empty.
    pub fn new(points: Vec<Point>) -> Result<Self, Error> {
        if points.is_empty() {
            return Err(Error::InvalidThreshold);
        }
        Ok(Self(points))
    }

    /// The committed points, constant term first.
    #[must_use]
    pub fn points(&self) -> &[Point] {
        &self.0
    }

    /// `C_0 = f(0) · G`.
    #[must_use]
    pub fn constant_term(&self) -> &Point {
        &self.0[0]
    }

    /// Reconstruction threshold of the committed polynomial.
    #[must_use]
    pub fn threshold(&self) -> usize {
        self.0.len()
    }

    /// `f(index) · G = Σ C_k · index^k`, computable by anyone.
    ///
    /// # Errors
    /// [`Error::InvalidIndex`] for index zero, [`Error::IdentityPoint`] if the result is the identity.
    pub fn evaluate(&self, index: MemberIndex) -> Result<Point, Error> {
        if index == 0 {
            return Err(Error::InvalidIndex);
        }
        let x = Scalar::from(index);
        let mut power = Scalar::one();
        let mut acc = Projective::zero();
        for commitment in &self.0 {
            acc += commitment.mul_public(&power);
            power *= x;
        }
        Point::try_from(acc)
    }

    /// Whether `share` is `f(index)` for the committed polynomial.
    #[must_use]
    pub fn verify_share(&self, index: MemberIndex, share: &SecretScalar) -> bool {
        match (self.evaluate(index), Point::generator_mul_secret(share)) {
            (Ok(expected), Ok(actual)) => expected == actual,
            _ => false,
        }
    }

    /// Rebuilds the commitments of a degree-`threshold - 1` polynomial from public evaluations `(i, f(i) · G)`, e.g. the
    /// verification shares in a committee manifest. The first `threshold` evaluations fix the polynomial (Lagrange
    /// interpolation in the exponent); every further evaluation must lie on it.
    ///
    /// # Errors
    /// Index-set errors, [`Error::InsufficientShares`] for fewer than `threshold` evaluations,
    /// [`Error::InvalidShare`] naming an evaluation off the polynomial, or [`Error::IdentityPoint`] for a degenerate
    /// coefficient.
    pub fn interpolate(
        threshold: usize,
        evaluations: &[(MemberIndex, Point)],
    ) -> Result<Self, Error> {
        let indices: Vec<MemberIndex> = evaluations.iter().map(|(i, _)| *i).collect();
        validate_index_set(&indices)?;
        if threshold == 0 {
            return Err(Error::InvalidThreshold);
        }
        if evaluations.len() < threshold {
            return Err(Error::InsufficientShares {
                needed: threshold,
                actual: evaluations.len(),
            });
        }
        let (basis, rest) = evaluations.split_at(threshold);
        let mut coefficients = vec![Projective::zero(); threshold];
        for (i, y) in basis {
            for (k, b) in lagrange_basis(*i, basis)?.iter().enumerate() {
                coefficients[k] += y.mul_public(b);
            }
        }
        let commitments = coefficients
            .into_iter()
            .map(Point::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map(Self)?;
        for (i, y) in rest {
            if commitments.evaluate(*i)? != *y {
                return Err(Error::InvalidShare(*i));
            }
        }
        Ok(commitments)
    }

    /// Point-wise sum of commitments (the committed polynomial is the sum of the polynomials).
    ///
    /// # Errors
    /// [`Error::InvalidThreshold`] if thresholds differ or the list is empty; [`Error::IdentityPoint`] on an identity
    /// sum.
    pub fn sum<'a>(all: impl IntoIterator<Item = &'a Commitments>) -> Result<Self, Error> {
        let mut iter = all.into_iter();
        let first = iter.next().ok_or(Error::InvalidThreshold)?;
        let mut acc: Vec<Projective> = first.0.iter().map(|p| p.as_affine().into_group()).collect();
        for next in iter {
            if next.0.len() != acc.len() {
                return Err(Error::InvalidThreshold);
            }
            for (sum, point) in acc.iter_mut().zip(&next.0) {
                *sum += point.as_affine();
            }
        }
        acc.into_iter()
            .map(Point::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}

/// Coefficients (constant first) of the Lagrange basis polynomial `L_i(x) = Π_{j ≠ i} (x - j) / (i - j)` over the
/// indices of `set`.
fn lagrange_basis(index: MemberIndex, set: &[(MemberIndex, Point)]) -> Result<Vec<Scalar>, Error> {
    let i = Scalar::from(index);
    let mut numerator = vec![Scalar::one()];
    let mut denominator = Scalar::one();
    for &(j, _) in set.iter().filter(|(j, _)| *j != index) {
        let j = Scalar::from(j);
        // numerator *= (x - j)
        let mut next = vec![Scalar::zero(); numerator.len() + 1];
        for (k, c) in numerator.iter().enumerate() {
            next[k + 1] += c;
            next[k] -= *c * j;
        }
        numerator = next;
        denominator *= i - j;
    }
    let inverse = denominator.inverse().ok_or(Error::InvalidIndex)?;
    Ok(numerator.into_iter().map(|c| c * inverse).collect())
}

/// Checks that `set` is non-empty, has no zero and no duplicate index.
///
/// # Errors
/// [`Error::InvalidIndex`] or [`Error::DuplicateIndex`].
pub fn validate_index_set(set: &[MemberIndex]) -> Result<(), Error> {
    if set.is_empty() {
        return Err(Error::InsufficientShares {
            needed: 1,
            actual: 0,
        });
    }
    for (position, index) in set.iter().enumerate() {
        if *index == 0 {
            return Err(Error::InvalidIndex);
        }
        if set[..position].contains(index) {
            return Err(Error::DuplicateIndex(*index));
        }
    }
    Ok(())
}

/// Lagrange coefficient at `x = 0` for `index` over `set`: `Π_{j ≠ i} j / (j - i)`.
///
/// # Errors
/// Index-set errors from [`validate_index_set`], or [`Error::InvalidIndex`] if `index` is not in `set`.
pub fn lagrange_coefficient(index: MemberIndex, set: &[MemberIndex]) -> Result<Scalar, Error> {
    validate_index_set(set)?;
    if !set.contains(&index) {
        return Err(Error::InvalidIndex);
    }
    let i = Scalar::from(index);
    let mut numerator = Scalar::one();
    let mut denominator = Scalar::one();
    for &j in set.iter().filter(|&&j| j != index) {
        let j = Scalar::from(j);
        numerator *= j;
        denominator *= j - i;
    }
    // Distinct non-zero indices make the denominator non-zero.
    let inverse = denominator.inverse().ok_or(Error::InvalidIndex)?;
    Ok(numerator * inverse)
}
