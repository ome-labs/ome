/// Why the device core refused or failed. Every variant carries a reason to show (`reason`, since Kotlin exceptions
/// already have a `message`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    /// An input is malformed: an offer, an id, a policy value.
    #[error("{reason}")]
    Invalid {
        /// What is wrong.
        reason: String,
    },
    /// A check refused the action: the review has failures, the offer expired, the signer refused.
    #[error("{reason}")]
    Refused {
        /// Why.
        reason: String,
    },
    /// The app did not hand over a sealing key: the user cancelled, or the platform denied it.
    #[error("{reason}")]
    Denied {
        /// What the platform said.
        reason: String,
    },
    /// The device is not paired, or the pairing is not complete.
    #[error("{reason}")]
    NotPaired {
        /// What is missing.
        reason: String,
    },
    /// The relays could not be reached or did not answer in time.
    #[error("{reason}")]
    Network {
        /// What failed.
        reason: String,
    },
    /// The app's storage failed, or a stored file is damaged.
    #[error("{reason}")]
    Storage {
        /// What failed.
        reason: String,
    },
}

impl FfiError {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid {
            reason: message.into(),
        }
    }

    pub(crate) fn refused(message: impl Into<String>) -> Self {
        Self::Refused {
            reason: message.into(),
        }
    }

    pub(crate) fn not_paired(message: impl Into<String>) -> Self {
        Self::NotPaired {
            reason: message.into(),
        }
    }

    pub(crate) fn network(message: impl Into<String>) -> Self {
        Self::Network {
            reason: message.into(),
        }
    }

    pub(crate) fn storage(message: impl Into<String>) -> Self {
        Self::Storage {
            reason: message.into(),
        }
    }
}

impl From<ome_wire::Error> for FfiError {
    fn from(error: ome_wire::Error) -> Self {
        Self::invalid(error.to_string())
    }
}

impl From<ome_crypto::Error> for FfiError {
    fn from(error: ome_crypto::Error) -> Self {
        Self::invalid(error.to_string())
    }
}

impl From<ome_protocol::Error> for FfiError {
    fn from(error: ome_protocol::Error) -> Self {
        Self::refused(error.to_string())
    }
}

impl From<ome_signer::Error> for FfiError {
    fn from(error: ome_signer::Error) -> Self {
        match error {
            ome_signer::Error::WrongSecret => Self::Denied {
                reason: "the signing key does not open with this sealing key".into(),
            },
            other => Self::refused(other.to_string()),
        }
    }
}

impl From<ome_net::Error> for FfiError {
    fn from(error: ome_net::Error) -> Self {
        Self::network(error.to_string())
    }
}

impl From<crate::StoreError> for FfiError {
    fn from(error: crate::StoreError) -> Self {
        match error {
            crate::StoreError::Denied { reason } => Self::Denied { reason },
            crate::StoreError::Failed { reason } => Self::Storage { reason },
        }
    }
}

/// Results of the core.
pub(crate) type Result<T, E = FfiError> = std::result::Result<T, E>;
