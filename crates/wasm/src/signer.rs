//! The member's approval key. It is generated and sealed here, and opened into an [`UnlockedSigner`] that lives in
//! WebAssembly memory: JavaScript sees the sealed document and the signatures, never the key.

use ome_api::{Hex32, HexBytes, Point, daemon::Secret};
use ome_signer::{SealedKey, SealingKey, Signer, SigningKey};
use ome_wire::Message;
use os_rng::SysRng;
use rand_core::UnwrapErr;
use wasm_bindgen::prelude::*;

use crate::{
    convert,
    error::Result,
    js::{export, from_js, seconds},
    types::Rotation,
};

fn rng() -> UnwrapErr<SysRng> {
    UnwrapErr(SysRng)
}

/// Runs `f` with the secret a param names.
fn with_secret<T>(
    secret: JsValue,
    f: impl FnOnce(&ome_signer::Secret<'_>) -> Result<T>,
) -> Result<T> {
    match from_js::<Secret>(secret)? {
        Secret::Passphrase { passphrase } => {
            let passphrase = zeroize::Zeroizing::new(passphrase);
            f(&ome_signer::Secret::Passphrase(&passphrase))
        }
        Secret::Key { key } => f(&ome_signer::Secret::Key(&SealingKey::from_bytes(key.0))),
    }
}

/// Generates a signing key and returns it sealed (the JSON document to store). A passphrase seals with Argon2id.
#[wasm_bindgen(js_name = generateSigner)]
pub fn generate_signer(
    #[wasm_bindgen(unchecked_param_type = "Secret")] secret: JsValue,
) -> Result<String, JsValue> {
    let run = || -> Result<String> {
        let mut rng = rng();
        let key = SigningKey::generate(&mut rng)?;
        let sealed = with_secret(secret, |secret| {
            Ok(SealedKey::seal(&key, secret, &mut rng)?)
        })?;
        Ok(sealed.to_json()?)
    };
    run().map_err(JsValue::from)
}

/// The public key of a sealed signer, readable without the secret.
#[wasm_bindgen(js_name = sealedPublicKey, unchecked_return_type = "Point")]
pub fn sealed_public_key(sealed: &str) -> Result<JsValue, JsValue> {
    export(|| {
        let key = SealedKey::from_json(sealed)?.public_key()?;
        Ok(Point::from_core(&key))
    })
}

/// The key a passphrase derives for this sealed signer, checked by opening it. Keep it behind the platform's
/// keychain or biometrics and unlock with `{ key }` without running Argon2id again.
#[wasm_bindgen(js_name = sealingKey, unchecked_return_type = "Hex32")]
pub fn sealing_key(sealed: &str, passphrase: &str) -> Result<JsValue, JsValue> {
    export(|| {
        let key = SealedKey::from_json(sealed)?.sealing_key(passphrase)?;
        Ok(Hex32(*key.to_bytes()))
    })
}

/// An opened signing key. It signs only proposals `ome-protocol` accepts, and is wiped when freed.
#[wasm_bindgen]
pub struct UnlockedSigner {
    key: SigningKey,
}

#[wasm_bindgen]
impl UnlockedSigner {
    /// Opens a sealed signer.
    pub fn open(
        sealed: &str,
        #[wasm_bindgen(unchecked_param_type = "Secret")] secret: JsValue,
    ) -> Result<Self, JsValue> {
        let run = || -> Result<Self> {
            let sealed = SealedKey::from_json(sealed)?;
            let key = with_secret(secret, |secret| Ok(sealed.open(secret)?))?;
            Ok(Self { key })
        };
        run().map_err(JsValue::from)
    }

    /// The public key, as it appears in the vault's signer set.
    #[wasm_bindgen(js_name = publicKey, unchecked_return_type = "Point")]
    pub fn public_key(&self) -> Result<JsValue, JsValue> {
        export(|| Ok(Point::from_core(&self.key.public_key())))
    }

    /// Approves a proposal after checking it (as `checkProposal`); a rotation is refused here, see `approveRotation`.
    /// Returns the approval to store with the keeper.
    #[wasm_bindgen(unchecked_return_type = "HexBytes")]
    pub fn approve(
        &self,
        #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Chain")] chain: JsValue,
        #[wasm_bindgen(unchecked_param_type = "HexBytes")] proposal: &str,
        #[wasm_bindgen(unchecked_param_type = "HexBytes")] payload: &str,
        now: f64,
    ) -> Result<JsValue, JsValue> {
        export(|| {
            let approval = ome_signer::approve(
                &self.key,
                &convert::vault(from_js(vault)?)?,
                convert::chain(from_js(chain)?)?,
                &convert::proposal(proposal)?,
                &convert::payload(payload)?,
                seconds(now)?,
                &mut rng(),
            )?;
            convert::body(&Message::Approval(approval))
        })
    }

    /// Approves a membership rotation the member reviewed: the payload must be the `rotate_membership` call for
    /// `rotation`, plus at most fee payment calls to one of `feePayers` (the fee payer contracts this member's own
    /// configuration trusts), and the rotation must pass the contract's rules and hold the vault's key.
    #[wasm_bindgen(js_name = approveRotation, unchecked_return_type = "HexBytes")]
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors ome_signer::approve_rotation"
    )]
    pub fn approve_rotation(
        &self,
        #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Chain")] chain: JsValue,
        #[wasm_bindgen(unchecked_param_type = "HexBytes")] proposal: &str,
        #[wasm_bindgen(unchecked_param_type = "HexBytes")] payload: &str,
        #[wasm_bindgen(unchecked_param_type = "Rotation")] rotation: JsValue,
        #[wasm_bindgen(js_name = feePayers, unchecked_param_type = "Hex32[]")] fee_payers: JsValue,
        now: f64,
    ) -> Result<JsValue, JsValue> {
        export(|| {
            let rotation: Rotation = from_js(rotation)?;
            let fee_payers: Vec<[u8; 32]> = from_js::<Vec<ome_api::Hex32>>(fee_payers)?
                .into_iter()
                .map(|address| address.0)
                .collect();
            let approval = ome_signer::approve_rotation(
                &self.key,
                &convert::vault(from_js(vault)?)?,
                convert::chain(from_js(chain)?)?,
                &convert::proposal(proposal)?,
                &convert::payload(payload)?,
                &crate::membership::rotation(rotation)?,
                &fee_payers,
                seconds(now)?,
                &mut rng(),
            )?;
            convert::body(&Message::Approval(approval))
        })
    }

    /// Approves a signer swap: the payload must be one `rotate_membership` call that replaces one member's key and
    /// changes nothing else, plus at most fee payment calls to one of `feePayers`. Returns the approval to store with
    /// the keeper.
    #[wasm_bindgen(js_name = approveSignerSwap, unchecked_return_type = "HexBytes")]
    pub fn approve_signer_swap(
        &self,
        #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Chain")] chain: JsValue,
        #[wasm_bindgen(unchecked_param_type = "HexBytes")] proposal: &str,
        #[wasm_bindgen(unchecked_param_type = "HexBytes")] payload: &str,
        #[wasm_bindgen(js_name = feePayers, unchecked_param_type = "Hex32[]")] fee_payers: JsValue,
        now: f64,
    ) -> Result<JsValue, JsValue> {
        export(|| {
            let fee_payers: Vec<[u8; 32]> = from_js::<Vec<ome_api::Hex32>>(fee_payers)?
                .into_iter()
                .map(|address| address.0)
                .collect();
            let (approval, _) = ome_signer::approve_signer_swap(
                &self.key,
                &convert::vault(from_js(vault)?)?,
                convert::chain(from_js(chain)?)?,
                &convert::proposal(proposal)?,
                &convert::payload(payload)?,
                &fee_payers,
                seconds(now)?,
                &mut rng(),
            )?;
            convert::body(&Message::Approval(approval))
        })
    }

    /// Rejects a proposal with a reason shown to the other members. Returns the rejection to store with the keeper.
    #[wasm_bindgen(unchecked_return_type = "HexBytes")]
    pub fn reject(
        &self,
        #[wasm_bindgen(unchecked_param_type = "VaultInput")] vault: JsValue,
        #[wasm_bindgen(unchecked_param_type = "HexBytes")] proposal: &str,
        reason: &str,
    ) -> Result<JsValue, JsValue> {
        export(|| -> Result<HexBytes> {
            let rejection = ome_signer::reject(
                &self.key,
                &convert::vault(from_js(vault)?)?,
                &convert::proposal(proposal)?,
                reason,
                &mut rng(),
            )?;
            convert::body(&Message::Rejection(rejection))
        })
    }
}
