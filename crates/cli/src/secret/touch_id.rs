//! Touch ID (macOS): the signer's sealing key as a login-keychain generic password, read only after a
//! `LocalAuthentication` check (Touch ID, with the account password as fallback) in this process.
#![allow(unsafe_code)]

use std::sync::mpsc;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAPolicy};
use security_framework::passwords;
use zeroize::Zeroizing;

use crate::error::{Error, Result};

const SERVICE: &str = "ome.signer";
/// `errSecItemNotFound`.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// Whether this Mac can run the device-owner check (Touch ID or the account password); never prompts.
pub(crate) fn available() -> Result<()> {
    // SAFETY: `LAContext` is a plain Foundation object; `new` has no preconditions.
    let context = unsafe { LAContext::new() };
    // SAFETY: a valid context and policy constant.
    unsafe { context.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication) }.map_err(|e| {
        Error::failed(format!(
            "Touch ID is not available: {}",
            e.localizedDescription()
        ))
    })
}

/// Runs the device-owner check; returns once the member authenticated or the check failed.
pub(crate) fn authenticate(reason: &str) -> Result<()> {
    available()?;
    // SAFETY: as in `available`.
    let context = unsafe { LAContext::new() };
    let policy = LAPolicy::DeviceOwnerAuthentication;
    let (tx, rx) = mpsc::channel::<std::result::Result<(), String>>();
    let reply = RcBlock::new(move |success: Bool, error: *mut NSError| {
        let outcome = if success.as_bool() {
            Ok(())
        } else {
            // SAFETY: LocalAuthentication passes a valid NSError (or null) that lives for the callback.
            Err(unsafe { error.as_ref() }.map_or_else(
                || "authentication failed".to_owned(),
                |e| e.localizedDescription().to_string(),
            ))
        };
        let _ = tx.send(outcome);
    });
    // SAFETY: the reply block is 'static and Send-safe (it only owns a channel sender); LocalAuthentication calls it
    // once on its own queue, so blocking this thread on the channel cannot deadlock.
    unsafe {
        context.evaluatePolicy_localizedReason_reply(policy, &NSString::from_str(reason), &reply);
    }
    rx.recv()
        .map_err(|_| Error::failed("Touch ID check ended without an answer"))?
        .map_err(|e| Error::failed(format!("Touch ID: {e}")))
}

/// Stores the sealing key for `account` (the profile directory), replacing any previous item.
pub(crate) fn store(account: &str, key_hex: &str) -> Result<()> {
    passwords::set_generic_password(SERVICE, account, key_hex.as_bytes())
        .map_err(|e| Error::failed(format!("could not store the key in the keychain: {e}")))
}

/// Authenticates, then reads the sealing key for `account`.
pub(crate) fn read(account: &str, reason: &str) -> Result<Zeroizing<String>> {
    authenticate(reason)?;
    let bytes = Zeroizing::new(
        passwords::get_generic_password(SERVICE, account)
            .map_err(|e| Error::failed(format!("could not read the key from the keychain: {e}")))?,
    );
    String::from_utf8(bytes.to_vec())
        .map(Zeroizing::new)
        .map_err(|_| {
            Error::failed("the keychain item is corrupt; run `ome signer touch-id on` again")
        })
}

/// Removes the item; a missing item is not an error.
pub(crate) fn remove(account: &str) -> Result<()> {
    match passwords::delete_generic_password(SERVICE, account) {
        Ok(()) => Ok(()),
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
        Err(e) => Err(Error::failed(format!(
            "could not remove the keychain item: {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    /// Run by hand on a Mac (`cargo test -p ome-cli -- --ignored`): the policy check never prompts.
    #[test]
    #[ignore = "needs a Mac with a login session"]
    fn policy_can_be_evaluated() {
        super::available().unwrap();
    }
}
