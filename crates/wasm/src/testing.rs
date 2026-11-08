//! Test helpers, built only with the `testing` feature into `@ome-labs/wasm/testing`: they handle raw secrets, which
//! production code never does.

use ome_api::{
    Hex32, Hex64, Point,
    keeper::{MemberPartials, Partial},
};
use ome_crypto::{
    SecretScalar, aztec, curve, schnorr,
    threshold::{
        ecdh::{AddressSecretShare, EcdhContext},
        shamir::Polynomial,
    },
};
use ome_wire::types::scalar;
use os_rng::SysRng;
use rand_core::UnwrapErr;
use wasm_bindgen::prelude::*;

use crate::{
    convert,
    error::{Result, WasmError},
    js::{export, from_js},
    types::DealtCommittee,
};

fn secret(hex: &str) -> Result<SecretScalar> {
    Ok(SecretScalar::new(curve::scalar_from_bytes(
        &convert::hex32(hex)?.0,
    )?))
}

/// Splits `secret` into `members` viewing shares with threshold `threshold`.
#[wasm_bindgen(js_name = dealShares, unchecked_return_type = "DealtCommittee")]
pub fn deal_shares(
    #[wasm_bindgen(unchecked_param_type = "Hex32")] secret: &str,
    threshold: u32,
    members: u32,
) -> Result<JsValue, JsValue> {
    export(|| {
        let secret = self::secret(secret)?;
        let group_key = Point::from_core(&ome_crypto::Point::generator_mul_secret(&secret)?);
        let threshold =
            usize::try_from(threshold).map_err(|e| WasmError::invalid(e.to_string()))?;
        let polynomial = Polynomial::random(secret, threshold, &mut UnwrapErr(SysRng))?;
        let mut shares = Vec::new();
        let mut verification_shares = Vec::new();
        for index in 1..=members {
            let share = polynomial.evaluate(index)?;
            verification_shares.push(Point::from_core(&ome_crypto::Point::generator_mul_secret(
                &share,
            )?));
            shares.push(Hex32(curve::scalar_to_bytes(share.expose())));
        }
        Ok(DealtCommittee {
            group_key,
            shares,
            verification_shares,
        })
    })
}

/// Member `index`'s partials for `inputs`, as its keeper would answer `ecdh.batch`.
#[wasm_bindgen(unchecked_return_type = "MemberPartials")]
pub fn partials(
    index: u32,
    #[wasm_bindgen(unchecked_param_type = "Hex32")] share: &str,
    #[wasm_bindgen(unchecked_param_type = "Hex32")] preaddress: &str,
    #[wasm_bindgen(unchecked_param_type = "Point")] ivpk: JsValue,
    epoch: u32,
    #[wasm_bindgen(unchecked_param_type = "Point[]")] inputs: JsValue,
) -> Result<JsValue, JsValue> {
    export(|| {
        let ivpk: Point = from_js(ivpk)?;
        let context = aztec::AddressContext::new(convert::field(preaddress)?, &ivpk.to_core()?)?;
        let share = AddressSecretShare::new(index, &secret(share)?, &context)?;
        let ecdh_context = EcdhContext {
            vault: context.address(),
            epoch,
        };
        let partials = share.partials(
            &convert::points(from_js(inputs)?)?,
            &ecdh_context,
            &mut UnwrapErr(SysRng),
        )?;
        Ok(MemberPartials {
            member: index,
            partials: partials
                .iter()
                .map(|p| Partial {
                    point: Point::from_core(&p.point),
                    challenge: Hex32(scalar(&p.proof.challenge)),
                    response: Hex32(scalar(&p.proof.response)),
                })
                .collect(),
        })
    })
}

/// The public key of a raw secret key.
#[wasm_bindgen(js_name = publicKey, unchecked_return_type = "Point")]
pub fn public_key(
    #[wasm_bindgen(js_name = secretKey, unchecked_param_type = "Hex32")] secret_key: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        Ok(Point::from_core(&ome_crypto::Point::generator_mul_secret(
            &secret(secret_key)?,
        )?))
    })
}

/// A Schnorr signature over any field element with a raw key.
#[wasm_bindgen(unchecked_return_type = "Hex64")]
pub fn sign(
    #[wasm_bindgen(js_name = secretKey, unchecked_param_type = "Hex32")] secret_key: &str,
    #[wasm_bindgen(unchecked_param_type = "Hex32")] message: &str,
) -> Result<JsValue, JsValue> {
    export(|| {
        let signature = schnorr::sign(
            &secret(secret_key)?,
            convert::field(message)?,
            &mut UnwrapErr(SysRng),
        )?;
        Ok(Hex64(signature.to_bytes()))
    })
}
