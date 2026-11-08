//! Conversions between the API shapes and the core types (feature `core`), shared by every service and binding that
//! serves these shapes. Points are checked to be on the curve, and fields to be canonical, on the way in.

use std::fmt;

use ome_crypto::Base;
use ome_protocol::vault::Config;
use ome_wire::{
    IdentityPublic,
    messages::ceremony,
    types::{WirePoint, field, parse_field},
};

use crate::{
    Hex32, Point,
    keeper::{Manifest, Member, VaultConfig},
};

/// A value the core refused: a point off the curve or a non-canonical field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidValue(pub String);

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvalidValue {}

impl From<ome_wire::Error> for InvalidValue {
    fn from(error: ome_wire::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<ome_crypto::Error> for InvalidValue {
    fn from(error: ome_crypto::Error) -> Self {
        Self(error.to_string())
    }
}

impl Hex32 {
    /// The field element, refusing values outside the field.
    ///
    /// # Errors
    /// A non-canonical encoding.
    pub fn to_field(self) -> Result<Base, InvalidValue> {
        Ok(parse_field(&self.0)?)
    }

    /// A field element's canonical encoding.
    #[must_use]
    pub fn from_field(value: &Base) -> Self {
        Self(field(value))
    }
}

impl Point {
    /// The curve point.
    ///
    /// # Errors
    /// Coordinates that are not a point on the curve.
    pub fn to_core(self) -> Result<ome_crypto::Point, InvalidValue> {
        Ok(ome_crypto::Point::try_from(&WirePoint::try_from(self)?)?)
    }

    /// A curve point's coordinates.
    #[must_use]
    pub fn from_core(point: &ome_crypto::Point) -> Self {
        Self::from(&WirePoint::from(point))
    }
}

impl From<&WirePoint> for Point {
    fn from(point: &WirePoint) -> Self {
        Self {
            x: point.x.into(),
            y: point.y.into(),
        }
    }
}

impl TryFrom<Point> for WirePoint {
    type Error = InvalidValue;

    fn try_from(point: Point) -> Result<Self, InvalidValue> {
        let point = Self {
            x: point.x.0,
            y: point.y.0,
        };
        ome_crypto::Point::try_from(&point)?;
        Ok(point)
    }
}

impl From<&ceremony::Manifest> for Manifest {
    fn from(m: &ceremony::Manifest) -> Self {
        Self {
            protocol_version: m.protocol_version,
            vault: m.vault.into(),
            epoch: m.epoch,
            threshold: m.threshold,
            viewing_threshold: m.viewing_threshold,
            preaddress: m.preaddress.into(),
            group_key: Point::from(&m.group_key),
            members: m
                .members
                .iter()
                .map(|member| Member {
                    index: member.index,
                    signer_key: Point::from(&member.signer_key),
                    verification_share: Point::from(&member.verification_share),
                    identity: member.identity.0.into(),
                    transport: member.transport.into(),
                    addresses: member.addresses.clone(),
                })
                .collect(),
        }
    }
}

impl TryFrom<Manifest> for ceremony::Manifest {
    type Error = InvalidValue;

    fn try_from(m: Manifest) -> Result<Self, InvalidValue> {
        Ok(Self {
            protocol_version: m.protocol_version,
            vault: m.vault.0,
            epoch: m.epoch,
            threshold: m.threshold,
            viewing_threshold: m.viewing_threshold,
            preaddress: m.preaddress.0,
            group_key: m.group_key.try_into()?,
            members: m
                .members
                .into_iter()
                .map(|member| {
                    Ok(ceremony::Member {
                        index: member.index,
                        signer_key: member.signer_key.try_into()?,
                        verification_share: member.verification_share.try_into()?,
                        identity: IdentityPublic(member.identity.0),
                        transport: member.transport.0,
                        addresses: member.addresses,
                    })
                })
                .collect::<Result<_, InvalidValue>>()?,
        })
    }
}

impl From<&Config> for VaultConfig {
    fn from(config: &Config) -> Self {
        Self {
            threshold: config.threshold,
            signer_count: config.signer_count,
            version: config.version,
            signer_set_commitment: Hex32::from_field(&config.signer_set_commitment),
            viewing_threshold: config.viewing_threshold,
            committee_commitment: Hex32::from_field(&config.committee_commitment),
        }
    }
}

impl TryFrom<VaultConfig> for Config {
    type Error = InvalidValue;

    fn try_from(config: VaultConfig) -> Result<Self, InvalidValue> {
        Ok(Self {
            threshold: config.threshold,
            signer_count: config.signer_count,
            version: config.version,
            signer_set_commitment: config.signer_set_commitment.to_field()?,
            viewing_threshold: config.viewing_threshold,
            committee_commitment: config.committee_commitment.to_field()?,
        })
    }
}
