//! `ome-relay`: run an untrusted Ome relay.
//!
//! `run` prints one JSON line `{"ready": {"peerId", "addresses"}}` on stdout once listening; logs go to stderr.

use std::{
    collections::HashSet,
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::{Parser, Subcommand, ValueEnum};
use libp2p::{Multiaddr, PeerId, identity};
use ome_relay::{Error, Limits, Relay, RelayConfig, Result, health, store::MailboxStore};
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "ome-relay",
    version,
    about = "Untrusted Ome relay: circuits and ciphertext mailboxes"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Runs the relay.
    Run(Run),
    /// Prints the relay's peer id (creating its identity if needed).
    PeerId {
        /// Data directory.
        #[arg(long, env = "OME_RELAY_DATA")]
        data_dir: PathBuf,
    },
}

#[derive(clap::Args)]
struct Run {
    /// Data directory (identity and mailbox database).
    #[arg(long, env = "OME_RELAY_DATA")]
    data_dir: PathBuf,
    /// Listen address (repeatable).
    #[arg(long, default_values_t = ["/ip4/0.0.0.0/udp/4001/quic-v1".to_owned(), "/ip4/0.0.0.0/tcp/4001".to_owned()])]
    listen: Vec<String>,
    /// Public address to announce (repeatable).
    #[arg(long)]
    external: Vec<String>,
    /// Health and metrics HTTP address.
    #[arg(long)]
    health: Option<SocketAddr>,
    /// Serve only these keeper peer ids (repeatable; default: anyone).
    #[arg(long)]
    allow_peer: Vec<String>,
    /// Longest envelope lifetime, days.
    #[arg(long, default_value_t = 14)]
    max_ttl_days: u64,
    /// Total mailbox storage, MiB.
    #[arg(long, default_value_t = 1024)]
    max_total_mib: u64,
    /// Log format.
    #[arg(long, value_enum, default_value_t = LogFormat::Text)]
    log_format: LogFormat,
}

#[derive(Clone, Copy, ValueEnum)]
enum LogFormat {
    Text,
    Json,
}

fn keypair(data: &Path) -> Result<identity::Keypair> {
    std::fs::create_dir_all(data)?;
    let path = data.join("identity.key");
    let mut seed = if let Ok(bytes) = std::fs::read(&path) {
        <[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| Error::Invalid("identity.key is corrupt".into()))?
    } else {
        let mut seed = [0u8; 32];
        os_rng::fill(&mut seed).map_err(|e| Error::Invalid(e.to_string()))?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path)?.write_all(&seed)?;
        seed
    };
    identity::Keypair::ed25519_from_bytes(&mut seed).map_err(|e| Error::Invalid(e.to_string()))
}

fn multiaddrs(values: &[String]) -> Result<Vec<Multiaddr>> {
    values
        .iter()
        .map(|v| v.parse().map_err(|e| Error::Invalid(format!("{v}: {e}"))))
        .collect()
}

fn print(value: &serde_json::Value) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{value}")?;
    stdout.flush()?;
    Ok(())
}

async fn run(args: Run) -> Result<()> {
    let keypair = keypair(&args.data_dir)?;
    let allow = if args.allow_peer.is_empty() {
        None
    } else {
        Some(
            args.allow_peer
                .iter()
                .map(|p| {
                    p.parse::<PeerId>()
                        .map_err(|e| Error::Invalid(format!("{p}: {e}")))
                })
                .collect::<Result<HashSet<_>>>()?,
        )
    };
    let limits = Limits {
        max_ttl_secs: args.max_ttl_days * 86_400,
        max_total_bytes: args.max_total_mib << 20,
        ..Limits::default()
    };
    let store = MailboxStore::open(&args.data_dir.join("mailbox.db"), limits)?;
    let config = RelayConfig {
        listen: multiaddrs(&args.listen)?,
        external: multiaddrs(&args.external)?,
        allow,
        limits,
        ..RelayConfig::default()
    };
    let relay = Relay::start(keypair, store, config).await?;
    let peer_id = relay.peer_id.to_string();
    if let Some(addr) = args.health {
        let bound = health::serve(
            addr,
            peer_id.clone(),
            Arc::clone(&relay.store),
            Arc::clone(&relay.metrics),
        )
        .await?;
        tracing::info!(%bound, "health endpoint");
    }
    print(&json!({ "ready": {
        "peerId": peer_id,
        "addresses": relay.addresses.iter().map(ToString::to_string).collect::<Vec<_>>(),
    }}))?;
    tracing::info!(peer = %relay.peer_id, "relay running");
    shutdown().await;
    tracing::info!("shutting down");
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = term.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::PeerId { data_dir } => {
            keypair(&data_dir).and_then(|k| print(&json!(k.public().to_peer_id().to_string())))
        }
        Command::Run(args) => {
            let filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into());
            let builder = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stderr);
            match args.log_format {
                LogFormat::Text => builder.init(),
                LogFormat::Json => builder.json().init(),
            }
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(Error::from)
                .and_then(|runtime| runtime.block_on(run(args)))
        }
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "ome-relay failed");
            std::process::ExitCode::FAILURE
        }
    }
}
