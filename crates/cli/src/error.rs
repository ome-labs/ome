//! CLI errors: a plain sentence, the fix, the cause, and the exit code they map to.

use std::fmt;

use ome_api::ErrorKind;
use ome_client::ClientError;
use serde_json::json;

/// What went wrong, which decides the exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exit {
    /// The daemon, keeper, node or chain failed; also a locked signer or a wrong passphrase (1).
    Failed,
    /// Bad arguments, a missing argument with prompts off, or an unknown or ambiguous reference (2).
    Usage,
    /// The profile's daemon is not running and could not be started (3).
    Daemon,
    /// Not possible yet: below threshold, keeper starting, node unreachable (4).
    NotReady,
    /// The member answered no, or pressed Ctrl-C at a prompt (5).
    Declined,
}

impl Exit {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Failed => 1,
            Self::Usage => 2,
            Self::Daemon => 3,
            Self::NotReady => 4,
            Self::Declined => 5,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Usage => "usage",
            Self::Daemon => "daemon",
            Self::NotReady => "not-ready",
            Self::Declined => "declined",
        }
    }

    /// The exit an API error kind maps to.
    pub(crate) const fn from_kind(kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::Invalid | ErrorKind::NotFound => Self::Usage,
            ErrorKind::NotReady => Self::NotReady,
            _ => Self::Failed,
        }
    }
}

/// A CLI error.
#[derive(Debug)]
pub(crate) struct Error {
    pub(crate) exit: Exit,
    /// One plain sentence.
    pub(crate) message: String,
    /// The next thing to run or do.
    pub(crate) hint: Option<String>,
    /// What caused it, shown with `--verbose`.
    pub(crate) cause: Option<String>,
    /// The API's error kind, when it came from the daemon.
    pub(crate) kind: Option<ErrorKind>,
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub(crate) fn new(exit: Exit, message: impl Into<String>) -> Self {
        Self {
            exit,
            message: message.into(),
            hint: None,
            cause: None,
            kind: None,
        }
    }

    pub(crate) fn failed(message: impl Into<String>) -> Self {
        Self::new(Exit::Failed, message)
    }

    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self::new(Exit::Usage, message)
    }

    pub(crate) fn declined() -> Self {
        Self::new(Exit::Declined, "Cancelled.")
    }

    /// A missing argument with prompts off, naming the flag.
    pub(crate) fn missing(what: &str, flag: &str) -> Self {
        Self::usage(format!("Missing {what}.")).hint(format!(
            "Pass {flag}, or run it in a terminal without --no-input to be asked."
        ))
    }

    #[must_use]
    pub(crate) fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    #[must_use]
    pub(crate) fn cause(mut self, cause: impl fmt::Display) -> Self {
        self.cause = Some(cause.to_string());
        self
    }

    /// Prefixes the message with what was being done: "Can't approve 4f2a: …".
    #[must_use]
    pub(crate) fn context(mut self, doing: impl fmt::Display) -> Self {
        self.message = format!("{doing}: {}", lower_first(&self.message));
        self
    }

    /// The `--json` error object printed on stderr.
    pub(crate) fn to_json(&self) -> serde_json::Value {
        let code = self.kind.map_or(self.exit.label(), ErrorKind::label);
        let mut error = json!({ "code": code, "message": self.message });
        if let Some(hint) = &self.hint {
            error["hint"] = json!(hint);
        }
        if let Some(cause) = &self.cause {
            error["cause"] = json!(cause);
        }
        json!({ "error": error })
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match (chars.next(), chars.clone().next()) {
        // Keep acronyms and names ("ECDH", "Bob") as they are.
        (Some(first), Some(second)) if first.is_uppercase() && second.is_lowercase() => {
            first.to_lowercase().chain(chars).collect()
        }
        _ => text.to_owned(),
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::Interrupted {
            return Self::declined();
        }
        Self::failed(error.to_string())
    }
}

impl From<ClientError> for Error {
    fn from(error: ClientError) -> Self {
        let kind = error.kind();
        let mut out = match &error {
            ClientError::Rpc(rpc) => {
                // A daemon message can carry a stack or a nested error after its first line: that is the cause.
                let (first, rest) = rpc.message.split_once('\n').unwrap_or((&rpc.message, ""));
                let mut out = Self::new(Exit::from_kind(kind), sentence(first));
                out.kind = Some(kind);
                if !rest.trim().is_empty() {
                    out.cause = Some(rest.trim().to_owned());
                }
                out
            }
            ClientError::Connect { .. } | ClientError::Discovery(_) => {
                Self::new(Exit::Daemon, "The daemon is not running.")
                    .hint("Start it with `ome daemon start`, or set up a profile with `ome init`.")
                    .cause(&error)
            }
            ClientError::Closed => Self::new(Exit::Daemon, "The daemon closed the connection.")
                .hint("Check it with `ome daemon logs`.")
                .cause(&error),
            _ => Self::failed(sentence(&error.to_string())),
        };
        out.hint = out.hint.or_else(|| hint_for(kind).map(str::to_owned));
        out
    }
}

/// The fix for an API error kind, when there is a general one.
fn hint_for(kind: ErrorKind) -> Option<&'static str> {
    match kind {
        ErrorKind::Locked => {
            Some("Unlock the signer with `ome unlock`, or set OME_PASSPHRASE_FILE.")
        }
        ErrorKind::Auth => Some("Check the passphrase, then try again."),
        ErrorKind::Alarm => Some(
            "The keeper stopped acting until someone looks: see `ome vault show` and `ome activity`.",
        ),
        ErrorKind::NotReady => Some("See what is missing with `ome status`."),
        _ => None,
    }
}

/// `the vault is unknown` → `The vault is unknown.`
fn sentence(text: &str) -> String {
    let mut chars = text.trim().chars();
    let Some(first) = chars.next() else {
        return "Something went wrong.".into();
    };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

#[cfg(test)]
mod tests {
    use ome_api::RpcError;

    use super::*;

    #[test]
    fn api_kinds_map_to_exit_codes() {
        let code = |kind| Exit::from_kind(kind).code();
        assert_eq!(code(ErrorKind::Invalid), 2);
        assert_eq!(code(ErrorKind::NotFound), 2);
        assert_eq!(code(ErrorKind::NotReady), 4);
        assert_eq!(code(ErrorKind::Locked), 1);
        assert_eq!(code(ErrorKind::Auth), 1);
        assert_eq!(code(ErrorKind::Alarm), 1);
        assert_eq!(Exit::Daemon.code(), 3);
        assert_eq!(Exit::Declined.code(), 5);
    }

    #[test]
    fn daemon_errors_read_as_sentences_with_their_fix() {
        let error = Error::from(ClientError::Rpc(RpcError::new(
            ErrorKind::Locked,
            "the signer is locked",
        )));
        assert_eq!(error.message, "The signer is locked.");
        assert_eq!(error.exit, Exit::Failed);
        assert!(
            error
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("ome unlock")
        );
        assert_eq!(error.to_json()["error"]["code"], "locked");
    }

    #[test]
    fn context_keeps_names_and_acronyms() {
        let error = Error::usage("It has 1 of 2 approvals.").context("Can't execute 4f2a");
        assert_eq!(
            error.message,
            "Can't execute 4f2a: it has 1 of 2 approvals."
        );
        let error = Error::usage("ECDH failed.").context("Can't read");
        assert_eq!(error.message, "Can't read: ECDH failed.");
    }

    #[test]
    fn a_missing_argument_names_its_flag() {
        let error = Error::missing("the amount", "<amount>");
        assert_eq!(error.exit.code(), 2);
        assert!(error.hint.unwrap_or_default().contains("<amount>"));
    }
}
