//! `ome-keeper`: run a member's keeper.
//!
//! The supervising app launches `ome-keeper run --secrets-stdin` and writes one JSON line with the
//! master key (kept in the OS keychain) and a fresh RPC token. A keeper on a member's own server uses a passphrase
//! (`OME_KEEPER_PASSPHRASE`) instead. When ready, `run` prints one JSON line on stdout with its identity, transport
//! key, addresses and RPC socket; logs go to stderr.

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::{Parser, Subcommand, ValueEnum};
use ome_keeper::{
    Error, Keeper, KeeperParts, Result, SystemClock,
    config::FileConfig,
    custody::{KeeperKeys, MasterKey, SecretStore, create_private_dir, write_private},
    rpc::{self, RpcToken},
    store::Store,
};
use ome_net::{Libp2pConfig, Libp2pTransport, Mailbox, Multiaddr, RelayMailbox};
use serde::Deserialize;
use serde_json::json;
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(
    name = "ome-keeper",
    version,
    about = "Ome keeper: custody of a member's viewing shares"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Creates the data directory and the keeper's keys, and prints its public identity.
    Init(Common),
    /// Prints the keeper's public identity and transport key.
    Identity(Common),
    /// Runs the keeper.
    Run(Run),
    /// Calls a method on a running keeper and prints the JSON result (operators and scripted tests).
    #[cfg(unix)]
    Rpc(RpcCall),
}

#[cfg(unix)]
#[derive(clap::Args)]
struct RpcCall {
    /// The keeper's RPC socket.
    #[arg(long)]
    socket: PathBuf,
    /// File holding the RPC token (`<data>/rpc.token` in passphrase mode).
    #[arg(long)]
    token_file: PathBuf,
    /// Method, e.g. `keeper.info`.
    method: String,
    /// Params as JSON.
    params: Option<String>,
}

#[derive(clap::Args)]
struct Common {
    /// Data directory.
    #[arg(long, env = "OME_KEEPER_DATA")]
    data_dir: PathBuf,
    /// Read `{"masterKey": "<hex>", "rpcToken": "<token>"}` from the first line of stdin.
    #[arg(long)]
    secrets_stdin: bool,
}

#[derive(clap::Args)]
struct Run {
    #[command(flatten)]
    common: Common,
    /// RPC socket path (default `<data>/keeper.sock`).
    #[arg(long)]
    rpc_socket: Option<PathBuf>,
    /// libp2p listen address (repeatable; default loopback QUIC and TCP on random ports).
    #[arg(long)]
    listen: Vec<String>,
    /// Relay multiaddr ending in `/p2p/<peer id>` (repeatable): circuits for unreachable members and mailboxes.
    #[arg(long)]
    relay: Vec<String>,
    /// Extra address to advertise (repeatable), e.g. a forwarded public address.
    #[arg(long)]
    external: Vec<String>,
    /// Log format.
    #[arg(long, value_enum, default_value_t = LogFormat::Text)]
    log_format: LogFormat,
}

#[derive(Clone, Copy, ValueEnum)]
enum LogFormat {
    Text,
    Json,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchSecrets {
    master_key: String,
    rpc_token: Option<String>,
}

struct Unlocked {
    secrets: SecretStore,
    token: Option<Zeroizing<String>>,
}

fn unlock(common: &Common) -> Result<Unlocked> {
    let data = &common.data_dir;
    create_private_dir(data)?;
    let (key, token) = if common.secrets_stdin {
        let mut line = Zeroizing::new(String::new());
        std::io::stdin().lock().read_line(&mut line)?;
        let launch: LaunchSecrets = serde_json::from_str(&line)
            .map_err(|_| Error::Invalid("stdin: expected launch secrets JSON".into()))?;
        (
            MasterKey::from_hex(&launch.master_key)?,
            launch.rpc_token.map(Zeroizing::new),
        )
    } else {
        let passphrase = Zeroizing::new(std::env::var("OME_KEEPER_PASSPHRASE").map_err(|_| {
            Error::Invalid("set OME_KEEPER_PASSPHRASE or use --secrets-stdin".into())
        })?);
        (
            MasterKey::from_passphrase(passphrase.as_bytes(), &salt(data)?)?,
            None,
        )
    };
    Ok(Unlocked {
        secrets: SecretStore::open(data.join("secrets"), &key)?,
        token,
    })
}

fn salt(data: &Path) -> Result<[u8; 16]> {
    let path = data.join("salt");
    if let Ok(bytes) = std::fs::read(&path) {
        return bytes
            .try_into()
            .map_err(|_| Error::DataDir("salt file is corrupt".into()));
    }
    let mut salt = [0u8; 16];
    os_rng::fill(&mut salt).map_err(|e| Error::Invalid(e.to_string()))?;
    write_private(&path, &salt)?;
    Ok(salt)
}

fn print(value: &serde_json::Value) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{value}")?;
    stdout.flush()?;
    Ok(())
}

fn identity_json(keys: &KeeperKeys) -> serde_json::Value {
    json!({
        "identity": format!("0x{}", hex::encode(keys.identity.public().0)),
        "transport": format!("0x{}", hex::encode(keys.transport_public.0)),
    })
}

async fn run(args: Run) -> Result<()> {
    let data = args.common.data_dir.clone();
    let file = FileConfig::load(&data.join("keeper.toml"))?;
    let unlocked = unlock(&args.common)?;
    let keys = KeeperKeys::load_or_create(&unlocked.secrets)?;
    let store = Store::open(&data.join("keeper.db"))?;
    let pick = |cli: Vec<String>, file: Option<Vec<String>>| {
        if cli.is_empty() {
            file.unwrap_or_default()
        } else {
            cli
        }
    };
    let mut p2p = Libp2pConfig::default();
    let listen = pick(args.listen, file.listen.clone());
    if !listen.is_empty() {
        p2p.listen = multiaddrs(&listen)?;
    }
    p2p.relays = multiaddrs(&pick(args.relay, file.relays.clone()))?;
    p2p.external = multiaddrs(&pick(args.external, file.external.clone()))?;
    let (transport, inbound) = Libp2pTransport::start(&keys.identity, p2p).await?;
    let addresses = transport.addresses().to_vec();
    let mailbox: Option<Arc<dyn Mailbox>> = if transport.relays().is_empty() {
        None
    } else {
        Some(Arc::new(RelayMailbox::new(Arc::clone(&transport))))
    };
    let socket = args
        .rpc_socket
        .or_else(|| file.rpc_socket.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| data.join("keeper.sock"));
    let token = if let Some(token) = unlocked.token {
        token
    } else {
        let mut bytes = [0u8; 32];
        os_rng::fill(&mut bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        let token = Zeroizing::new(hex::encode(bytes));
        write_private(&data.join("rpc.token"), token.as_bytes())?;
        token
    };
    let public = identity_json(&keys);
    let keeper = Keeper::start(KeeperParts {
        keys,
        secrets: unlocked.secrets,
        store,
        transport: Box::new(Arc::clone(&transport)),
        inbound,
        clock: Arc::new(SystemClock),
        config: file.keeper_config(),
        addresses: addresses.clone(),
        mailbox,
    })?;
    let mut ready = public;
    ready["peerId"] = json!(transport.peer_id().to_string());
    ready["addresses"] = json!(addresses);
    ready["rpcSocket"] = json!(socket.display().to_string());
    write_private(&data.join("keeper.json"), ready.to_string().as_bytes())?;

    let server = tokio::spawn(rpc::serve(keeper, socket.clone(), RpcToken::new(&token)));
    // `serve` binds before accepting; give it a moment, then announce readiness.
    tokio::task::yield_now().await;
    while !socket_ready(&socket) {
        if server.is_finished() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    print(&json!({ "ready": ready }))?;
    tracing::info!("keeper running");
    tokio::select! {
        result = server => result.map_err(|e| Error::Invalid(e.to_string()))??,
        () = shutdown() => tracing::info!("shutting down"),
    }
    transport.stop();
    let _ = std::fs::remove_file(&socket);
    Ok(())
}

fn multiaddrs(values: &[String]) -> Result<Vec<Multiaddr>> {
    values
        .iter()
        .map(|v| v.parse().map_err(|e| Error::Invalid(format!("{v}: {e}"))))
        .collect()
}

/// `ome-keeper rpc`: one authenticated call over the socket.
#[cfg(unix)]
fn rpc_call(call: &RpcCall) -> Result<()> {
    use std::{io::BufReader, os::unix::net::UnixStream};

    let token = Zeroizing::new(std::fs::read_to_string(&call.token_file)?);
    let params: serde_json::Value = match &call.params {
        Some(p) => serde_json::from_str(p).map_err(|e| Error::Invalid(format!("params: {e}")))?,
        None => serde_json::Value::Null,
    };
    let stream = UnixStream::connect(&call.socket)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    let requests = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "session.auth", "params": { "token": token.trim() } }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": call.method, "params": params }),
    ];
    for request in &requests {
        writeln!(writer, "{request}")?;
    }
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(Error::Transport("keeper closed the connection".into()));
        }
        let reply: serde_json::Value =
            serde_json::from_str(&line).map_err(|e| Error::Invalid(e.to_string()))?;
        if let Some(error) = reply.get("error") {
            return Err(Error::Invalid(error.to_string()));
        }
        if reply.get("id") == Some(&json!(2)) {
            return print(&reply["result"]);
        }
    }
}

#[cfg(unix)]
fn socket_ready(path: &Path) -> bool {
    path.exists()
}

#[cfg(not(unix))]
fn socket_ready(_: &Path) -> bool {
    true
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

fn init_logging(format: LogFormat) {
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    match format {
        LogFormat::Text => builder.init(),
        LogFormat::Json => builder.json().init(),
    }
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Init(common) | Command::Identity(common) => {
            init_logging(LogFormat::Text);
            unlock(&common)
                .and_then(|unlocked| KeeperKeys::load_or_create(&unlocked.secrets))
                .and_then(|keys| print(&identity_json(&keys)))
        }
        #[cfg(unix)]
        Command::Rpc(call) => rpc_call(&call),
        Command::Run(args) => {
            init_logging(args.log_format);
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
            tracing::error!(%error, "ome-keeper failed");
            std::process::ExitCode::FAILURE
        }
    }
}
