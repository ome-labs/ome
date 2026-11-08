use ome_crypto::{Point, SecretScalar, schnorr};
use rand_core::CryptoRng;

use crate::{Error, Message, Signer};

/// A member's approval signing key. It is generated on the device, leaves it only sealed, and cannot be cloned,
/// printed or serialized.
pub struct SigningKey {
    secret: SecretScalar,
    public: Point,
}

impl SigningKey {
    /// Generates a new key.
    ///
    /// # Errors
    /// [`Error::Crypto`] in the negligible case of a zero scalar.
    pub fn generate(rng: &mut impl CryptoRng) -> Result<Self, Error> {
        Self::from_secret(SecretScalar::random(rng))
    }

    pub(crate) fn from_secret(secret: SecretScalar) -> Result<Self, Error> {
        let public = Point::generator_mul_secret(&secret)?;
        Ok(Self { secret, public })
    }

    pub(crate) fn secret(&self) -> &SecretScalar {
        &self.secret
    }
}

impl Signer for SigningKey {
    fn public_key(&self) -> Point {
        self.public
    }

    fn sign<R: CryptoRng>(
        &self,
        message: &Message,
        rng: &mut R,
    ) -> Result<schnorr::Signature, Error> {
        Ok(schnorr::sign(&self.secret, message.value(), rng)?)
    }
}
