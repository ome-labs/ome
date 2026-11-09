# The command line (`ome`)

`crates/cli` (`ome-cli`) builds `ome`, a member's command line. It is a client of the member's daemon (`omed`,
`architecture/daemon.md`) over the daemon API (`architecture/api.md`), through `ome-client`. It holds no keys: the
signer stays sealed in the daemon, and viewing shares stay in the keeper.

## Principles

- **Vault work only.** `ome` founds, pays, reviews, votes and changes members. Ordinary accounts, deployments, Fee
  Juice and authwits stay with `aztec-wallet`; where a member needs one of those, `ome` prints the `aztec-wallet`
  line (`ome vault receive`).
- **Speak Aztec.** The flags `aztec-wallet` and `aztec` use keep their names: `-n/--node-url` (`AZTEC_NODE_URL`),
  `-p/--prover native|wasm|none` (`PXE_PROVER`), `-ca/--contract-address`, `--args`, `--no-wait`, `-v/--verbose`.
  `aztec-wallet`'s typed aliases (`accounts:…`, `contracts:…`) are read everywhere an address is taken. clap's short
  flags are one letter, so `ome` rewrites a lone `-ca` to `--contract-address` before parsing (not after `--`).
- **Guide, never trap.** In a terminal every missing argument is asked for; with prompts off it is an error that names
  the flag.
- **Clear signing.** Nothing is signed without the review screen, rebuilt and checked on this device.
- **Progress as it happens.** Long steps (DKG, deployment, proving, reshares) are named and timed while they run.

## The command tree

```text
SET UP      init · status · doctor · config get|set|unset · profile list|show|use|remove
            daemon start|stop|restart|logs · keeper status|connect|use-local|logs · completions <shell>
SIGNER      unlock [--for 15m] [--touch-id] · lock · signer status|touch-id on|off
VAULTS      vault create|join|list|show|use|alias|members|peers|health|audit|receive|balance|retire
            balance [token] · activity [--follow] · counterparties
            token add|list|remove|deploy-test
PROPOSALS   propose transfer <amount> <token> <to> | call <fn> -ca <contract> --args … | cancel <proposal>
            (each with --memo, --expires 24h, --dry-run)
            proposals [--status open|ready|done|all] · proposal list|show|review|approve|reject|execute
            approve <proposal> · reject <proposal> [--reason] · execute <proposal> [--no-wait]
MEMBERS     member request|change|join|status|swap-key
DEVICES     device pair|list|enroll|forward|revoke
LIVE        watch
```

`ome` alone shows the status and the next step in a terminal, and the help elsewhere. The daily loop (`approve`,
`reject`, `execute`, `propose`, `balance`) is also at the top level; `proposal approve` and the others are the same
commands. People never meet lobbies: `vault create` and `vault join` drive `lobby.*` and `founding.*`.

On screen, `k` is "approvals to spend" and `t_v` "members to read"; `--approvals/-k` and `--readers/-t` take them.

### Globals

| Flag                            | Meaning                                                                              |
| ------------------------------- | ------------------------------------------------------------------------------------ |
| `-P, --profile` (`OME_PROFILE`) | the profile (below)                                                                  |
| `--vault` (`OME_VAULT`)         | the vault (below)                                                                    |
| `--json`                        | one JSON value on stdout (the API's shapes); errors as JSON on stderr; never prompts |
| `--plain`                       | one record per line, tab-separated; never prompts                                    |
| `--no-input`                    | never prompt                                                                         |
| `-y, --yes`                     | answer yes to confirmations; never to authentication, never to a typed confirmation  |
| `-q, --quiet`                   | only the essential line (an id, a hash)                                              |
| `--no-color`                    | also `NO_COLOR`, `TERM=dumb`, or a pipe                                              |
| `-v, --verbose`                 | full values instead of short ones, causes of errors                                  |

### Which profile and which vault

Nobody should have to type `--profile` or `--vault` again and again.

- **Profile:** `--profile`, else `OME_PROFILE` (set it once per terminal when several members share a machine), else
  the one `ome profile use` chose (`<home>/current-profile`), else the only profile set up. With several and none
  chosen, a terminal asks; scripts get an error naming the three ways to choose. `ome init` names a new profile after
  its member (`Alice Smith` → `alice-smith`), so `ome init` twice sets up two members.
- **Vault:** `--vault`, else `OME_VAULT`, else the only vault. With several, a terminal asks every time, starting on
  the one `ome vault use` chose; scripts use that one.

### References

One resolver (`src/refs.rs`) takes every address argument:

| Form                                  | Resolves to                                                 |
| ------------------------------------- | ----------------------------------------------------------- |
| `0x…` (64 hex digits)                 | itself                                                      |
| `accounts:<name>`, `contracts:<name>` | `aztec-wallet`'s alias store, through `aztec.aliases`       |
| `vault:<name>`                        | a vault by alias or name                                    |
| a bare name                           | all of the above; the same address found twice is one match |
| a token symbol or name                | a token the vault tracks                                    |
| a hex prefix                          | a proposal, vault or device by the start of its id          |

A bare name that matches several things is asked about in a terminal, and otherwise fails naming the typed forms
(`vault:treasury, accounts:treasury`). Ome keeps no address book: name people with
`aztec-wallet alias accounts bob 0x…`, and reviews say the name is that alias, which nobody verified. The arguments of `propose call` are resolved the same way when they use a typed
form; anything else is passed as typed.

## Output

- Results go to stdout; prompts, steps, notes and errors go to stderr, so `ome … > file` keeps only results.
- The look is Clack's (`cliclack`): a left rail, `◇` sections, spinners that resolve into a timed line. Without a
  terminal there are no spinners, but finished steps still print their line, so logs read like the screen.
- Colour is for meaning only (`src/ui/theme.rs`): green done, yellow attention, red refused, cyan what can be typed,
  dim for detail, bold for amounts.
- Addresses are shortened (`0x2b1f…9e04`) with labels first; amounts are in token units with base units on review
  screens; times are relative (`in 5h 12m`); a call argument that fits in 128 bits is shown as its number.
- Tables (`comfy-table`) have no borders and dim headers; `--plain` prints their rows tab-separated without headers.
- After a change of state, the next command is printed (`next: ome execute 4f2a`), with `--profile` when one was
  named.

## Clear signing and safety

`ome approve` and `ome reject` show the review the daemon rebuilt on this device (`proposals.review`,
`src/ui/review.rs`):

1. what happens if it executes (who is paid, how much, the memo, who pays the fee: never the vault in public);
2. the calls, as rebuilt;
3. the checks: the calls hash to the outer hash, the config is current, the deadline, the simulation, and a warning
   when the recipient's name is the member's own `aztec-wallet` alias, or when it has none;
4. the votes so far;
5. the exact message signed and its formula.

A failed check refuses the approval (rejecting stays possible); a failed simulation is a warning. `--yes` skips only
the final question.

Thresholds that can strand the funds for good (every keeper needed to read, or every key needed to spend: see
`architecture/recovery.md`) are shown when a vault is created or its members change, and need a deliberate yes
(`--yes` in scripts); the other policy warnings are shown only.

Irreversible actions ask the member to type the vault's (or profile's) name, and take `--confirm <name>` in scripts:
`vault retire`, `member change` with a removal, `device enroll`, `profile remove`. `--yes` does not satisfy them.

## The signer's secret

`src/secret/` decides where the secret for one signature comes from, in this order:

1. none, while the daemon's signer is unlocked;
2. Touch ID (macOS, in a terminal, when turned on): the sealing key from the login keychain (service `ome.signer`,
   account the profile directory), read after a `LocalAuthentication` check; a marker file `touch-id` in the profile
   says it is on without prompting;
3. `OME_PASSPHRASE_FILE`, for automation;
4. a prompt that never echoes.

Without any of them the command fails as `locked` before anything is signed. The passphrase is never an argument or
a flag value. If the unlock window closes between the check and the signature, the vote is retried once with a secret.

`ome signer touch-id on` checks Touch ID, asks the passphrase once, derives the sealing key (`signer.sealingKey`) and
stores it in the keychain.

## The daemon

- Commands find the profile's daemon through `ome-client` discovery. When the profile is set up but its daemon is not
  running, they start it (`src/launch.rs`): `OME_DAEMON`, else `<home>/bin/omed`, else the checkout's
  `packages/daemon/src/omed.ts` with Node. It runs detached in its own process group with its output in
  `daemon.log`.
- `ome init` starts the daemon for a new profile, points it at the node (restarting it if the URL changed: the daemon
  reads it as it starts), checks the node answers, then creates the profile.
- `ome config set node-url` restarts the daemon for the same reason.
- When the keeper's master key is sealed under the signer (no OS keychain), the keeper and vaults start at the first
  unlock. A command that needs a vault and finds none opens the signer for one second to start them.

## `ome watch`

The only full-screen view (`ratatui`, `src/watch.rs`): a header (vault, thresholds, node, keeper, signer), the
vault's proposals, its activity, and the keys. It redraws on the daemon's events and every 10 seconds.

| Key           | Does                |
| ------------- | ------------------- |
| `↑ ↓` / `j k` | select a proposal   |
| enter         | review it           |
| `a` / `r`     | approve / reject it |
| `x`           | execute it          |
| `p`           | propose a payment   |
| `v`           | next vault          |
| `R`           | refresh             |
| `q` / Esc     | quit                |

Acting leaves the full screen and runs the ordinary command, review included, then comes back: nothing is signed from
a screen that hides the review.

## Exit codes

| Code | When                                                                                      |
| ---- | ----------------------------------------------------------------------------------------- |
| 0    | done                                                                                      |
| 1    | failed: daemon, keeper, node or chain errors; `locked`, `auth` and `alarm`                |
| 2    | usage: bad or missing arguments, unknown or ambiguous references (`invalid`, `not-found`) |
| 3    | the daemon is not running and could not be started, or the profile is not set up          |
| 4    | not ready yet (`not-ready`): below threshold, keeper starting, node unreachable           |
| 5    | declined at a prompt, or Ctrl-C                                                           |

## Layout

| Path             | What it is                                                               |
| ---------------- | ------------------------------------------------------------------------ |
| `src/args.rs`    | the clap tree and the grouped help                                       |
| `src/context.rs` | profile, output mode, prompting, the daemon session                      |
| `src/launch.rs`  | finding, starting and stopping `omed`                                    |
| `src/refs.rs`    | the reference resolver                                                   |
| `src/secret/`    | the secret sources and Touch ID                                          |
| `src/ui/`        | theme, prompts, steps, tables, QR codes, descriptions, the review screen |
| `src/commands/`  | one module per group                                                     |
| `src/watch.rs`   | `ome watch`                                                              |

## Installing

`pnpm cli:install` builds `ome` and `ome-keeper` in release, copies them into `<home>/bin` (`$OME_HOME/bin`, default
`~/.ome/bin`), writes an `omed` wrapper that runs this checkout's daemon with Node, and prints the `PATH` line.

## Tests

| Test                                    | What it checks                                                                                                                                                                        |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| unit tests in `src/`                    | parsing, `-ca`, references, amounts, durations, exit codes, secret sources, plain rows                                                                                                |
| snapshots (`insta`, `src/ui/snapshots`) | the review screen and a table, without colour                                                                                                                                         |
| `src/watch.rs`                          | the full-screen view rendered on ratatui's test backend                                                                                                                               |
| `tests/fake_daemon.rs`                  | the binary against a stand-in daemon on the real socket protocol: JSON and plain output, the passphrase coming only from `OME_PASSPHRASE_FILE`, exit codes, ambiguous prefixes        |
| `scripts/test-cli.ts` (`pnpm test:cli`) | three members through `ome` only against the local network: profiles, founding a 2-of-3 vault, a payment approved by two (with an early execute refused as exit 4), removing a member |

Measured on the local network with fake proofs: `pnpm test:cli`, 4 tests in 47 s.

## Not built yet

- `--payment`: the fee payer is the profile's setting (`ome config set fee-payer`), not a per-proposal flag.
- `--wait-for-status`: `ome execute` waits for the block, or with `--no-wait` for the hash only.
- `-c <artifact>`: `propose call` uses the artifact the daemon registered for the contract.
- Links on transaction hashes to an explorer, and pairing a phone in `pnpm test:cli`.
