//! The command tree (`architecture/cli.md`): noun-verb groups, with the daily multisig loop also as top-level verbs.
//! Flags shared with `aztec-wallet` keep its names: `-n/--node-url`, `-p/--prover`, `-ca/--contract-address`,
//! `--args`, `--no-wait`, `-v/--verbose`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum, builder::styling};

const STYLES: styling::Styles = styling::Styles::styled()
    .header(styling::AnsiColor::Green.on_default().bold())
    .usage(styling::AnsiColor::Green.on_default().bold())
    .literal(styling::AnsiColor::Cyan.on_default())
    .placeholder(styling::AnsiColor::Cyan.on_default().dimmed());

/// The command list in groups, as `ome --help` shows it.
const COMMANDS: &str = "\
\x1b[1;32mSet up:\x1b[0m
  \x1b[36minit\x1b[0m            guided first run: profile, passphrase, network; starts the daemon
  \x1b[36mstatus\x1b[0m          profile, daemon, keeper, node, signer, vaults, and the next step
  \x1b[36mdoctor\x1b[0m          check Node, binaries, the node, relays and the keychain
  \x1b[36mconfig\x1b[0m          get, set or unset a setting
  \x1b[36mprofile\x1b[0m         list, show, use or remove profiles
  \x1b[36mdaemon\x1b[0m          start, stop, restart or read the logs of omed
  \x1b[36mkeeper\x1b[0m          your keeper: status, a keeper on your server, logs
  \x1b[36mcompletions\x1b[0m     shell completions

\x1b[1;32mSigner:\x1b[0m
  \x1b[36munlock\x1b[0m          keep the signer open for a while
  \x1b[36mlock\x1b[0m            close it now
  \x1b[36msigner\x1b[0m          status, Touch ID

\x1b[1;32mVaults:\x1b[0m
  \x1b[36mvault\x1b[0m           create, join, list, show, use, alias, members, peers, health, audit, receive, retire
  \x1b[36mbalance\x1b[0m         what the vault holds
  \x1b[36mactivity\x1b[0m        proposals and received transfers
  \x1b[36mcounterparties\x1b[0m  who paid the vault, and who should handshake again
  \x1b[36mtoken\x1b[0m           add, list, remove or deploy a test token
  \x1b[36mcontract\x1b[0m        add, list or remove contracts proposals call; their functions

\x1b[1;32mProposals:\x1b[0m
  \x1b[36mpropose\x1b[0m         a transfer, a contract call, or a cancel
  \x1b[36mproposals\x1b[0m       the vault's proposals
  \x1b[36mproposal\x1b[0m        show or review one
  \x1b[36mapprove\x1b[0m         review, then sign an approval
  \x1b[36mreject\x1b[0m          review, then sign a rejection
  \x1b[36mexecute\x1b[0m         prove and send a ready proposal

\x1b[1;32mMembers and devices:\x1b[0m
  \x1b[36mmember\x1b[0m          join requests, membership changes, key swaps
  \x1b[36mdevice\x1b[0m          pair a phone, move your key to it

\x1b[1;32mLive:\x1b[0m
  \x1b[36mwatch\x1b[0m           full-screen view of a vault; act with single keys";

const AFTER: &str = "\
References: an address can be 0x…, accounts:<name> or contracts:<name> (aztec-wallet's aliases), vault:<name>,
or a bare name; a token its symbol; a proposal a unique prefix of its id. Name people with
`aztec-wallet alias accounts <name> <address>`: ome keeps no address book of its own.

Run `ome <command> --help` for a command's flags. Docs: https://github.com/ome-labs/ome/blob/main/architecture/cli.md";

/// `ome`: an Ome member's command line. Vault work only: everything else stays with `aztec-wallet`.
#[derive(Debug, Parser)]
#[command(
    name = "ome",
    version = ome_api::RELEASE_VERSION,
    about = "Ome: a private k-of-n multisig on Aztec, from the terminal",
    styles = STYLES,
    help_template = "{about}\n\n{usage-heading} {usage}\n\n{before-help}\x1b[1;32mOptions:\x1b[0m\n{options}{after-help}",
    before_help = COMMANDS,
    after_help = AFTER,
    disable_help_subcommand = true,
    subcommand_value_name = "COMMAND",
)]
pub(crate) struct Cli {
    #[command(flatten)]
    pub(crate) global: Global,
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// Flags every command takes.
#[derive(Debug, Args)]
#[command(next_help_heading = "Global options")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent command-line switches"
)]
pub(crate) struct Global {
    /// The profile to act as (else `ome profile use`'s, the only one, or you are asked).
    #[arg(
        short = 'P',
        long,
        global = true,
        env = "OME_PROFILE",
        value_name = "NAME"
    )]
    pub(crate) profile: Option<String>,
    /// The vault to act on (default: the profile's chosen or only vault).
    #[arg(long, global = true, env = "OME_VAULT", value_name = "VAULT")]
    pub(crate) vault: Option<String>,
    /// One JSON value on stdout; errors as JSON on stderr. Never prompts.
    #[arg(long, global = true, conflicts_with = "plain")]
    pub(crate) json: bool,
    /// One record per line, tab-separated. Never prompts.
    #[arg(long, global = true)]
    pub(crate) plain: bool,
    /// Never prompt: a missing argument is an error naming its flag.
    #[arg(long, global = true)]
    pub(crate) no_input: bool,
    /// Answer yes to confirmations (never to authentication).
    #[arg(short = 'y', long, global = true)]
    pub(crate) yes: bool,
    /// Only the essential line: an id, a hash.
    #[arg(short, long, global = true)]
    pub(crate) quiet: bool,
    /// No colour (also `NO_COLOR`, `TERM=dumb`, or a pipe).
    #[arg(long, global = true)]
    pub(crate) no_color: bool,
    /// Full values, timings and causes.
    #[arg(short, long, global = true)]
    pub(crate) verbose: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Guided first run: profile, passphrase, network; starts the daemon.
    Init(InitArgs),
    /// Profile, daemon, keeper, node, signer, vaults, and the next step.
    Status,
    /// The fee payer this member trusts and its Fee Juice.
    Fees,
    /// Check Node, binaries, the node, relays and the keychain.
    Doctor(NodeArgs),
    /// Get, set or unset a setting.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Update Ome the way it was installed, then restart this profile's daemon on the new release.
    Update,
    /// List, show, use or remove profiles.
    #[command(subcommand)]
    Profile(ProfileCommand),
    /// Start, stop, restart or read the logs of the daemon (omed).
    #[command(subcommand)]
    Daemon(DaemonCommand),
    /// Your keeper: its status, a keeper on your own server, or the daemon's own.
    #[command(subcommand)]
    Keeper(KeeperCommand),
    /// Print shell completions.
    Completions {
        /// bash, zsh, fish, elvish or powershell.
        shell: clap_complete::Shell,
    },

    /// Keep the signer open for a while.
    Unlock(UnlockArgs),
    /// Close the signer now.
    Lock,
    /// The signer: status, Touch ID.
    #[command(subcommand)]
    Signer(SignerCommand),

    /// Create, join, list and inspect vaults.
    #[command(subcommand)]
    Vault(VaultCommand),
    /// What the vault holds.
    Balance(BalanceArgs),
    /// Proposals and received transfers, newest first.
    Activity(ActivityArgs),
    /// Who paid the vault, and who should handshake with it again.
    Counterparties,
    /// A payment request: an ome-pay: link and QR code naming the vault, and what you ask for.
    Request(RequestArgs),
    /// The tokens the vault tracks.
    #[command(subcommand)]
    Token(TokenCommand),
    /// The contracts proposals can call.
    #[command(subcommand)]
    Contract(ContractCommand),

    /// Propose a transfer, a contract call, or a cancel.
    #[command(subcommand)]
    Propose(ProposeCommand),
    /// The vault's proposals.
    Proposals(ListArgs),
    /// Show, review or act on one proposal.
    #[command(subcommand)]
    Proposal(ProposalCommand),
    /// Review a proposal, then sign an approval.
    Approve(ProposalArg),
    /// Review a proposal, then sign a rejection.
    Reject(RejectArgs),
    /// Prove and send a ready proposal from this device.
    Execute(ExecuteArgs),

    /// Join requests, membership changes and key swaps.
    #[command(subcommand)]
    Member(MemberCommand),
    /// Pair a phone, and move your signing key to it.
    #[command(subcommand)]
    Device(DeviceCommand),
    /// Apps allowed to use a vault: their transactions become proposals.
    #[command(subcommand)]
    Apps(AppsCommand),

    /// Full-screen view of a vault: proposals, votes, activity; act with single keys.
    Watch,
}

/// How proofs are made, in `aztec-wallet`'s words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Prover {
    /// Prove on this device (the native prover).
    Native,
    /// Prove on this device (same as native: the daemon picks the backend).
    Wasm,
    /// No proofs: only the local network accepts them.
    None,
}

#[derive(Debug, Args)]
pub(crate) struct NodeArgs {
    /// The Aztec node's JSON-RPC URL.
    #[arg(short = 'n', long, env = "AZTEC_NODE_URL", value_name = "URL")]
    pub(crate) node_url: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    /// Your name, as the other members see it.
    #[arg(long)]
    pub(crate) name: Option<String>,
    #[command(flatten)]
    pub(crate) node: NodeArgs,
    /// Prove on this device (native, wasm), or not at all (none: the local network only).
    #[arg(short = 'p', long, env = "PXE_PROVER", value_enum)]
    pub(crate) prover: Option<Prover>,
    /// A relay multiaddr ending in /p2p/<peer id>; repeat for more.
    #[arg(long = "relay", value_name = "MULTIADDR")]
    pub(crate) relays: Vec<String>,
    /// Unlock with Touch ID from now on (macOS).
    #[arg(long)]
    pub(crate) touch_id: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum ConfigKey {
    /// The Aztec node URL.
    #[value(name = "node-url")]
    NodeUrl,
    /// native, wasm or none.
    Prover,
    /// Relay multiaddrs, comma-separated.
    Relays,
    /// Salt of the `SponsoredFPC` instance that pays your transactions (0 for the canonical one).
    #[value(name = "fee-payer")]
    FeePayer,
    /// Longest unlock window: 15m, 1h, 24h.
    #[value(name = "unlock-timeout")]
    UnlockTimeout,
    /// Also write vault aliases into aztec-wallet's store: on or off.
    #[value(name = "aztec-wallet.sync")]
    AztecWalletSync,
    /// aztec-wallet's data directory, when not ~/.aztec/wallet.
    #[value(name = "aztec-wallet.dir")]
    AztecWalletDir,
    /// Answer app connections: on or off.
    Apps,
    /// Check once a day for a newer release of Ome: on or off.
    #[value(name = "update-check")]
    UpdateCheck,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Every setting, or one.
    Get {
        /// The setting.
        key: Option<ConfigKey>,
    },
    /// Change a setting.
    Set {
        /// The setting.
        key: ConfigKey,
        /// Its new value.
        value: String,
    },
    /// Put a setting back to its default.
    Unset {
        /// The setting.
        key: ConfigKey,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProfileCommand {
    /// Every profile in this Ome home.
    List,
    /// One profile: its member, account and directory.
    Show {
        /// The profile (default: the current one).
        name: Option<String>,
    },
    /// Make a profile the one commands use.
    Use {
        /// The profile (asked for when left out).
        name: Option<String>,
    },
    /// Your name, as every vault's members see it.
    Rename {
        /// The new name.
        name: String,
    },
    /// Stop a profile's daemon and delete everything it holds; refused while it is in a vault.
    Remove {
        /// The profile.
        name: String,
        /// The profile's name, to confirm without a prompt.
        #[arg(long, value_name = "NAME")]
        confirm: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum DaemonCommand {
    /// Start the daemon in the background.
    Start {
        /// Run it in this terminal instead, until Ctrl-C.
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the daemon and its keeper.
    Stop,
    /// Stop, then start again.
    Restart,
    /// The daemon's log.
    Logs(LogArgs),
    /// Run the daemon as a login service (launchd or systemd), so your keeper keeps answering.
    InstallService,
    /// Stop the service and remove it.
    UninstallService,
}

#[derive(Debug, Args)]
pub(crate) struct LogArgs {
    /// Keep printing new lines.
    #[arg(short, long)]
    pub(crate) follow: bool,
    /// Lines from the end.
    #[arg(short = 'n', long, default_value_t = 50)]
    pub(crate) lines: usize,
}

#[derive(Debug, Subcommand)]
pub(crate) enum KeeperCommand {
    /// The keeper's state.
    Status,
    /// Use a keeper on your own server (`ome-keeper run --rpc-listen`) over TLS.
    Connect {
        /// Its host:port.
        address: Option<String>,
        /// The SHA-256 fingerprint the keeper printed.
        #[arg(long)]
        fingerprint: Option<String>,
        /// A file holding its RPC token (`<data>/rpc.token` on the server).
        #[arg(long, value_name = "FILE")]
        token_file: Option<PathBuf>,
    },
    /// Use the daemon's own keeper again.
    UseLocal,
    /// The local keeper's log.
    Logs(LogArgs),
}

#[derive(Debug, Args)]
pub(crate) struct UnlockArgs {
    /// How long: 15m, 1h, 8h (at most the unlock timeout).
    #[arg(long = "for", value_name = "DURATION")]
    pub(crate) duration: Option<String>,
    /// Use Touch ID this time (macOS).
    #[arg(long)]
    pub(crate) touch_id: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Toggle {
    On,
    Off,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SignerCommand {
    /// Whether the signer is locked, and Touch ID.
    Status,
    /// Unlock with Touch ID instead of typing the passphrase (macOS).
    TouchId {
        /// on or off.
        state: Toggle,
    },
    /// Change the passphrase: the signing key and the keeper's sealed key are re-sealed, the key is unchanged.
    Passphrase,
}

#[derive(Debug, Subcommand)]
pub(crate) enum VaultCommand {
    /// Create a vault with co-founders: a wizard that prints the invite and founds the vault.
    Create(CreateArgs),
    /// The lobby you host: show it, remove a joiner, or change its size and thresholds before it starts.
    #[command(subcommand)]
    Lobby(LobbyCommand),
    /// Rename the vault for every member (no proposal needed).
    Rename {
        /// The new name.
        name: String,
    },
    /// Join a vault a co-founder is creating.
    Join {
        /// The invite (ome-invite-v0:…).
        invite: Option<String>,
    },
    /// Your vaults.
    List,
    /// One vault in full: config, committee, deployment.
    Show {
        /// The vault (default: --vault, or the chosen one).
        #[arg(value_name = "VAULT")]
        target: Option<String>,
    },
    /// Choose the vault scripts use and prompts start on.
    Use {
        /// The vault.
        #[arg(value_name = "VAULT")]
        target: Option<String>,
    },
    /// Give the vault a short name you can type.
    Alias {
        /// Lowercase letters, digits, - and _.
        alias: Option<String>,
    },
    /// The members and the viewing committee.
    Members,
    /// Whether each member's keeper is reachable.
    Peers,
    /// Whether the vault can still read, how many keepers it can lose, and how to recover one.
    Health,
    /// Who asked your keeper for threshold reads, and what it answered.
    Audit {
        /// Most recent entries.
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// The vault's address, as text and QR, and how to fund it from aztec-wallet.
    Receive,
    /// What the vault holds (same as `ome balance`).
    Balance(BalanceArgs),
    /// Retire the share your keeper kept after a rotation removed you.
    Retire {
        /// The vault's name, to confirm without a prompt.
        #[arg(long, value_name = "NAME")]
        confirm: Option<String>,
    },
}

#[derive(Debug, Args)]
pub(crate) struct CreateArgs {
    /// The vault's name.
    #[arg(long)]
    pub(crate) name: Option<String>,
    /// How many members found it, you included.
    #[arg(long)]
    pub(crate) members: Option<u8>,
    /// Approvals needed to spend (k).
    #[arg(long, short = 'k')]
    pub(crate) approvals: Option<u8>,
    /// Members needed to read (`t_v`), at least 2.
    #[arg(long, short = 't')]
    pub(crate) readers: Option<u8>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum LobbyCommand {
    /// Your open lobbies: who has joined, and the invite.
    Show,
    /// Remove a joiner from the lobby you host; the old invite stops working and a new one is printed.
    Remove {
        /// The joiner's name or keeper identity.
        member: String,
    },
    /// Change the lobby's size or thresholds before it starts.
    Update {
        /// How many members found it, you included.
        #[arg(long)]
        members: Option<u8>,
        /// Approvals needed to spend (k).
        #[arg(long, short = 'k')]
        approvals: Option<u8>,
        /// Members needed to read (`t_v`).
        #[arg(long, short = 't')]
        readers: Option<u8>,
    },
}

#[derive(Debug, Args)]
pub(crate) struct RequestArgs {
    /// How much, in whole tokens (`25.5`).
    pub(crate) amount: Option<String>,
    /// Which token (symbol or address); the vault's only token when it tracks one.
    pub(crate) token: Option<String>,
    /// What it is for.
    #[arg(long)]
    pub(crate) memo: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct BalanceArgs {
    /// One token (symbol or address).
    pub(crate) token: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ActivityArgs {
    /// Keep printing what happens.
    #[arg(short, long)]
    pub(crate) follow: bool,
    /// Most recent entries.
    #[arg(short = 'n', long, default_value_t = 20)]
    pub(crate) limit: usize,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TokenCommand {
    /// Track a token, so its balance and transfers show.
    Add(TokenAddArgs),
    /// The tracked tokens and their balances.
    List,
    /// Stop tracking a token.
    Remove {
        /// Symbol or address.
        token: Option<String>,
    },
    /// Deploy a test token and mint to the vault (local network only).
    DeployTest,
}

#[derive(Debug, Args)]
pub(crate) struct TokenAddArgs {
    /// The token contract: 0x…, or contracts:<name>.
    pub(crate) token: Option<String>,
    /// For a token whose instance was never published: its deployment salt.
    #[arg(long, requires_all = ["deployer", "admin", "token_name", "symbol", "decimals"])]
    pub(crate) salt: Option<String>,
    /// Its deployer (0 for a universal deployment).
    #[arg(long, requires = "salt")]
    pub(crate) deployer: Option<String>,
    /// The constructor's admin.
    #[arg(long, requires = "salt")]
    pub(crate) admin: Option<String>,
    /// The constructor's name.
    #[arg(long = "token-name", requires = "salt")]
    pub(crate) token_name: Option<String>,
    /// The constructor's symbol.
    #[arg(long, requires = "salt")]
    pub(crate) symbol: Option<String>,
    /// The constructor's decimals.
    #[arg(long, requires = "salt")]
    pub(crate) decimals: Option<u8>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ContractCommand {
    /// Register a contract from its artifact, checked against the class at its address.
    Add(ContractAddArgs),
    /// The contracts proposals can call: added ones, tokens and vaults.
    List,
    /// Forget an added contract.
    Remove {
        /// 0x…, or contracts:<name>.
        contract: Option<String>,
    },
    /// A contract's functions and their parameter types.
    Functions {
        /// 0x…, or contracts:<name>.
        contract: Option<String>,
        /// Also list utility (read-only) functions.
        #[arg(long)]
        utility: bool,
    },
}

#[derive(Debug, Args)]
pub(crate) struct ContractAddArgs {
    /// The artifact's JSON file, as `aztec compile` writes it.
    pub(crate) artifact: Option<String>,
    /// The contract: 0x…, or contracts:<name>.
    pub(crate) address: Option<String>,
    /// For a contract whose instance was never published: its deployment salt.
    #[arg(long, requires_all = ["deployer", "initialization_hash", "immutables_hash", "public_keys"])]
    pub(crate) salt: Option<String>,
    /// Its deployer (0 for a universal deployment).
    #[arg(long, requires = "salt")]
    pub(crate) deployer: Option<String>,
    /// Its initialization hash.
    #[arg(long, requires = "salt")]
    pub(crate) initialization_hash: Option<String>,
    /// Its immutables hash.
    #[arg(long, requires = "salt")]
    pub(crate) immutables_hash: Option<String>,
    /// Its public keys, as Aztec serializes them.
    #[arg(long, requires = "salt")]
    pub(crate) public_keys: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ProposeOptions {
    /// A note for the members.
    #[arg(long)]
    pub(crate) memo: Option<String>,
    /// How long members have to approve it: 24h, 3d (whole hours, at most 30d).
    #[arg(long, value_name = "DURATION", default_value = "24h")]
    pub(crate) expires: String,
    /// Simulate only; propose nothing.
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProposeCommand {
    /// Pay a token privately from the vault.
    Transfer {
        /// In token units: 250, 0.5.
        amount: Option<String>,
        /// Symbol or address of a tracked token.
        token: Option<String>,
        /// The recipient: 0x…, accounts:<name> (aztec-wallet), or a name.
        to: Option<String>,
        #[command(flatten)]
        options: ProposeOptions,
    },
    /// Call a contract from the vault, in `aztec-wallet send`'s shape.
    Call {
        /// The function.
        function: Option<String>,
        /// The contract: 0x…, or contracts:<name> (also -ca).
        #[arg(long = "contract-address", value_name = "ADDRESS")]
        contract: Option<String>,
        /// The function's arguments; references such as accounts:<name> are resolved.
        #[arg(long, num_args = 0.., allow_hyphen_values = true, value_name = "ARG")]
        args: Vec<String>,
        /// Let the called contract make one private call for the vault in this transaction (a private authwit), given
        /// as "<contract> <function> [args…]" with references resolved. Repeat for each; members approve it with the
        /// call.
        #[arg(long = "authwit", value_name = "CALL")]
        authwits: Vec<String>,
        #[command(flatten)]
        options: ProposeOptions,
    },
    /// Cancel a proposal by spending its nonce: itself a proposal.
    Cancel {
        /// The proposal to cancel.
        proposal: Option<String>,
        #[command(flatten)]
        options: ProposeOptions,
    },
    /// Change how many members must approve a payment.
    Threshold {
        /// The new approvals to spend, from 1 to the member count.
        approvals: u32,
        #[command(flatten)]
        options: ProposeOptions,
    },
    /// Void every approval collected so far: open proposals need their votes again.
    Invalidate {
        #[command(flatten)]
        options: ProposeOptions,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum StatusFilter {
    /// Collecting votes, ready, or blocked (waiting to be cancelled).
    #[default]
    Open,
    /// Ready to execute.
    Ready,
    /// Waiting for your vote.
    NeedsMe,
    /// Executed, cancelled, expired or stale.
    Done,
    /// Everything.
    All,
}

impl StatusFilter {
    /// The daemon's filter.
    pub(crate) const fn filter(self) -> ome_api::daemon::ProposalFilter {
        use ome_api::daemon::ProposalFilter;
        match self {
            Self::Open => ProposalFilter::Open,
            Self::Ready => ProposalFilter::Ready,
            Self::NeedsMe => ProposalFilter::NeedsMe,
            Self::Done => ProposalFilter::Done,
            Self::All => ProposalFilter::All,
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct ListArgs {
    /// Which proposals.
    #[arg(long, value_enum, default_value_t)]
    pub(crate) status: StatusFilter,
}

#[derive(Debug, Args)]
pub(crate) struct ProposalArg {
    /// The proposal: a unique prefix of its id.
    pub(crate) proposal: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct RejectArgs {
    /// The proposal.
    pub(crate) proposal: Option<String>,
    /// Why, for the other members.
    #[arg(long)]
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ExecuteArgs {
    /// The proposal.
    pub(crate) proposal: Option<String>,
    /// Return once the transaction is sent, without waiting for its block.
    #[arg(long)]
    pub(crate) no_wait: bool,
    /// Execute even while another member is: both prove, and only one transaction can land.
    #[arg(long)]
    pub(crate) force: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProposalCommand {
    /// The vault's proposals (same as `ome proposals`).
    List(ListArgs),
    /// A proposal and its votes.
    Show(ProposalArg),
    /// Rebuild and check a proposal on this device, without voting.
    Review(ProposalArg),
    /// Same as `ome approve`.
    Approve(ProposalArg),
    /// Same as `ome reject`.
    Reject(RejectArgs),
    /// Same as `ome execute`.
    Execute(ExecuteArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum MemberCommand {
    /// A join request to hand to a member of the vault you should join.
    Request,
    /// Add and remove members and set new thresholds; prints the plan to share.
    Change(ChangeArgs),
    /// Take part in a planned change someone shared with you.
    Join {
        /// The plan (ome-rotation-v0:…).
        plan: Option<String>,
    },
    /// Where membership changes are.
    Status {
        /// One change, by id prefix.
        change: Option<String>,
    },
    /// Replace one member's signing key (a lost or new device), keeping everything else.
    SwapKey {
        /// The member: their name or index.
        member: Option<String>,
        /// The new key, 0x<x>:0x<y>, from the member's new device.
        key: Option<String>,
    },
}

#[derive(Debug, Args)]
pub(crate) struct ChangeArgs {
    /// A newcomer's join request (from `ome member request`); repeat for more.
    #[arg(long = "add", value_name = "REQUEST")]
    pub(crate) add: Vec<String>,
    /// A member to remove, by name or index; repeat for more.
    #[arg(long = "remove", value_name = "MEMBER")]
    pub(crate) remove: Vec<String>,
    /// Approvals needed to spend afterwards (k).
    #[arg(long, short = 'k')]
    pub(crate) approvals: Option<u32>,
    /// Members needed to read afterwards (`t_v`).
    #[arg(long, short = 't')]
    pub(crate) readers: Option<u32>,
    /// The vault's name, to confirm a removal without a prompt.
    #[arg(long, value_name = "NAME")]
    pub(crate) confirm: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AppsCommand {
    /// The apps allowed to use a vault.
    List,
    /// Let an app, by its web origin (scheme, host and port), use this vault. Reads and simulations are answered at
    /// once; every transaction it sends becomes a proposal the members approve.
    Allow {
        /// The app's origin.
        origin: String,
        /// A name to show for it.
        #[arg(long)]
        name: Option<String>,
    },
    /// Stop an app from using any vault.
    Revoke {
        /// The app's origin.
        origin: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum DeviceCommand {
    /// Pair a phone: shows a QR code and waits until the phone answers.
    Pair,
    /// Your paired devices.
    List,
    /// Move your signing key in this vault to a paired device (a proposal).
    Enroll {
        /// The device: its name or identity prefix.
        device: Option<String>,
        /// The vault's name, to confirm without a prompt.
        #[arg(long, value_name = "NAME")]
        confirm: Option<String>,
    },
    /// Send a proposal to the device that holds your key, now.
    Forward(ProposalArg),
    /// Unpair a device.
    Revoke {
        /// The device: its name or identity prefix.
        device: Option<String>,
    },
}

/// `aztec-wallet` spells the contract flag `-ca`; clap's short flags are one letter, so it is rewritten here.
pub(crate) fn normalize(args: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut passthrough = false;
    for arg in args {
        if arg == "--" {
            passthrough = true;
        }
        if !passthrough && arg == "-ca" {
            out.push("--contract-address".to_owned());
        } else {
            out.push(arg);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    fn parse(line: &str) -> Cli {
        let args = normalize(
            std::iter::once("ome")
                .chain(line.split_whitespace())
                .map(String::from),
        );
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn the_tree_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn propose_call_speaks_aztec_wallet() {
        let cli =
            parse("propose call transfer_in_public -ca contracts:usdc --args accounts:bob 10 0");
        let Some(Command::Propose(ProposeCommand::Call {
            function,
            contract,
            args,
            ..
        })) = cli.command
        else {
            panic!("not a call");
        };
        assert_eq!(function.as_deref(), Some("transfer_in_public"));
        assert_eq!(contract.as_deref(), Some("contracts:usdc"));
        assert_eq!(args, ["accounts:bob", "10", "0"]);
    }

    #[test]
    fn propose_call_grants_authwits() {
        let cli = Cli::try_parse_from([
            "ome",
            "propose",
            "call",
            "swap_exact_tokens_for_tokens",
            "--contract-address",
            "contracts:amm",
            "--authwit",
            "contracts:jade transfer_to_public vault: contracts:amm 100 0",
            "--args",
            "contracts:jade",
            "contracts:opal",
            "100",
            "90",
            "0",
        ])
        .unwrap();
        let Some(Command::Propose(ProposeCommand::Call { args, authwits, .. })) = cli.command
        else {
            panic!("not a call");
        };
        assert_eq!(
            authwits,
            ["contracts:jade transfer_to_public vault: contracts:amm 100 0"]
        );
        assert_eq!(args.len(), 5);
    }

    #[test]
    fn update_and_its_check_parse() {
        assert!(matches!(parse("update").command, Some(Command::Update)));
        assert!(matches!(
            parse("config set update-check off").command,
            Some(Command::Config(ConfigCommand::Set {
                key: ConfigKey::UpdateCheck,
                ..
            }))
        ));
    }

    #[test]
    fn globals_go_anywhere() {
        let cli = parse("approve 4f2a --json --vault treasury -y");
        assert!(cli.global.json && cli.global.yes);
        assert_eq!(cli.global.vault.as_deref(), Some("treasury"));
        assert!(matches!(
            cli.command,
            Some(Command::Approve(ProposalArg { proposal: Some(_) }))
        ));
        assert!(Cli::try_parse_from(["ome", "--json", "--plain", "status"]).is_err());
    }

    #[test]
    fn transfer_takes_its_arguments_in_order() {
        let cli = parse("propose transfer 250 USDC bob --memo rent --expires 3d");
        let Some(Command::Propose(ProposeCommand::Transfer {
            amount,
            token,
            to,
            options,
        })) = cli.command
        else {
            panic!("not a transfer");
        };
        assert_eq!(
            (amount.as_deref(), token.as_deref(), to.as_deref()),
            (Some("250"), Some("USDC"), Some("bob"))
        );
        assert_eq!(options.memo.as_deref(), Some("rent"));
        assert_eq!(options.expires, "3d");
    }

    #[test]
    fn request_takes_an_amount_a_token_and_a_memo() {
        let cli = parse("request 25.5 USDC --memo rent");
        let Some(Command::Request(args)) = cli.command else {
            panic!("not a request");
        };
        assert_eq!(
            (
                args.amount.as_deref(),
                args.token.as_deref(),
                args.memo.as_deref()
            ),
            (Some("25.5"), Some("USDC"), Some("rent"))
        );
    }

    #[test]
    fn threshold_takes_the_new_count() {
        let cli = parse("propose threshold 3 --dry-run");
        let Some(Command::Propose(ProposeCommand::Threshold { approvals, options })) = cli.command
        else {
            panic!("not a threshold change");
        };
        assert_eq!(approvals, 3);
        assert!(options.dry_run);
    }

    #[test]
    fn ca_after_a_double_dash_is_an_argument() {
        assert_eq!(
            normalize(["-ca".into(), "--".into(), "-ca".into()]),
            ["--contract-address", "--", "-ca"]
        );
    }
}
