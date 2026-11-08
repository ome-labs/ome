//! `ome-device`: a signing device in a directory, for development and the daemon's end-to-end tests. It runs the same
//! core as the phone apps, but its sealing keys are plain files: never use it for a real vault.
//!
//! ```text
//! ome-device --dir <dir> create <name> | info | pair <offer> [--wait <secs>] | sync | reviews
//!            | approve <id> | reject <id> <reason> | unpair
//! ```
//!
//! Every command prints one JSON value.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::{Parser, Subcommand};
use ome_ffi::{DeviceStore, FfiError, KeyPurpose, OmeDevice, Review, StoreError};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "ome-device",
    about = "A development signing device in a directory"
)]
struct Args {
    /// The device's directory.
    #[arg(long)]
    dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Creates the device's keys.
    Create {
        /// Display name.
        name: String,
    },
    /// Who the device is.
    Info,
    /// Pairs with the keeper that made the offer.
    Pair {
        /// The `ome-pair-v0:` offer.
        offer: String,
        /// Seconds to wait for the keeper's answer.
        #[arg(long, default_value_t = 120)]
        wait: u32,
    },
    /// Fetches reviews and answers.
    Sync,
    /// Received proposals.
    Reviews,
    /// Approves a proposal.
    Approve {
        /// Proposal id.
        id: String,
    },
    /// Rejects a proposal.
    Reject {
        /// Proposal id.
        id: String,
        /// Reason shown to the members.
        reason: String,
    },
    /// Ends the pairing.
    Unpair,
}

/// Files in a directory, with the sealing keys as two more files.
struct DirStore(PathBuf);

impl DirStore {
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

fn failed(error: impl std::fmt::Display) -> StoreError {
    StoreError::Failed {
        reason: error.to_string(),
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

impl DeviceStore for DirStore {
    fn read(&self, name: String) -> Result<Option<Vec<u8>>, StoreError> {
        match std::fs::read(self.path(&name)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(failed(error)),
        }
    }

    fn write(&self, name: String, bytes: Vec<u8>) -> Result<(), StoreError> {
        write_private(&self.path(&name), &bytes).map_err(failed)
    }

    fn remove(&self, name: String) -> Result<(), StoreError> {
        match std::fs::remove_file(self.path(&name)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(failed(error)),
            _ => Ok(()),
        }
    }

    fn sealing_key(&self, purpose: KeyPurpose) -> Result<Vec<u8>, StoreError> {
        let name = match purpose {
            KeyPurpose::Transport => "sealing-transport.key",
            KeyPurpose::Signing => "sealing-signing.key",
        };
        if let Some(key) = self.read(name.into())? {
            return Ok(key);
        }
        let mut key = vec![0u8; 32];
        os_rng::fill(&mut key).map_err(failed)?;
        self.write(name.into(), key.clone())?;
        Ok(key)
    }
}

fn review_json(review: &Review) -> Value {
    json!({
        "id": review.id,
        "vault": review.vault,
        "kind": format!("{:?}", review.kind),
        "summary": review.summary,
        "signable": review.signable,
        "status": format!("{:?}", review.status),
        "outcome": review.outcome,
        "failed": review.checks.iter().filter(|c| !c.passed)
            .map(|c| json!({ "name": c.name, "detail": c.detail })).collect::<Vec<_>>(),
        "calls": review.calls.iter().map(|c| json!({
            "to": c.to, "function": c.function, "label": c.label, "feePayment": c.fee_payment,
            "arguments": c.arguments.iter().map(|a| json!({ "kind": a.kind, "value": a.value })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

async fn run(args: Args) -> Result<Value, FfiError> {
    std::fs::create_dir_all(&args.dir).map_err(|e| FfiError::Storage {
        reason: e.to_string(),
    })?;
    let store: Arc<dyn DeviceStore> = Arc::new(DirStore(args.dir));
    let device = match &args.command {
        Command::Create { name } => OmeDevice::create(store, name.clone())?,
        _ => OmeDevice::open(store)?,
    };
    Ok(match args.command {
        Command::Create { .. } | Command::Info => {
            let info = device.info();
            json!({
                "name": info.name, "identity": info.identity, "transport": info.transport,
                "signerKey": { "x": info.signer_key.x, "y": info.signer_key.y }, "peerId": info.peer_id,
            })
        }
        Command::Pair { offer, wait } => {
            let pairing = device.pair(offer, wait).await?;
            json!({ "keeper": pairing.keeper, "member": pairing.member, "network": pairing.network })
        }
        Command::Sync => {
            let result = device.sync().await?;
            json!({ "reviews": result.reviews, "outcomes": result.outcomes, "unpaired": result.unpaired })
        }
        Command::Reviews => Value::Array(device.reviews()?.iter().map(review_json).collect()),
        Command::Approve { id } => review_json(&device.approve(id).await?),
        Command::Reject { id, reason } => review_json(&device.reject(id, reason).await?),
        Command::Unpair => {
            device.unpair().await?;
            json!({ "unpaired": true })
        }
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let (value, code) = match run(args).await {
        Ok(value) => (value, std::process::ExitCode::SUCCESS),
        Err(error) => (
            json!({ "error": error.to_string(), "kind": format!("{error:?}").split(' ').next().unwrap_or("") }),
            std::process::ExitCode::FAILURE,
        ),
    };
    let _ = writeln!(std::io::stdout(), "{value}");
    code
}
