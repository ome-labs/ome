# Ome

A private k-of-n multisig on [Aztec](https://aztec.network) with threshold viewing: `k` members to spend, `t` members
to read, and nobody ever holds the vault's viewing key.

- **Spending** is enforced by the vault's account contract: a payment runs only when the proof carries valid
  signatures from at least `k` registered members. Signers, thresholds and the committee are never written to public
  storage or public arguments.
- **Reading** is threshold decryption: each member's keeper holds a share of the vault's incoming viewing secret, and
  the wallet decrypts by asking `t` keepers for their part of each Diffie-Hellman exchange, each part with a DLEQ
  proof. The secret is never assembled anywhere.
- **Membership changes** reshare the viewing key and update the signers together. The vault keeps its address, and a
  removed member's keeper cannot read anything that arrives afterwards.

[Website](https://ome.network) · [Architecture](architecture/README.md) · [CLI guide](architecture/cli.md) ·
[Releases](https://github.com/ome-labs/ome/releases) · [Security](SECURITY.md)

## Install

macOS on Apple silicon, or Linux (x64, arm64). Any of:

```sh
curl -fsSL https://ome.network/install.sh | sh     # into ~/.ome; fetches Node 24 if you have none
brew install ome-labs/tap/ome                       # Homebrew, with its Node 24
npm install --global @ome-labs/ome                  # Node 24 or later
```

Each installs the same three programs: `ome`, the command line; `omed`, the daemon `ome` starts for each profile; and
`ome-keeper`, which holds your share of each vault's viewing key. `ome doctor` checks the install and says how to
update it. Every release is built in public CI with provenance: see [verifying a release](architecture/release.md).

## Quick start

```sh
ome init                          # your profile: name, network (Testnet or a local network), passphrase
ome vault create                  # found a vault with co-founders: prints the invite
ome vault join <invite>           # each co-founder
ome vault receive                 # the vault's address, and the aztec-wallet line to fund it
ome propose transfer 250 USDC bob # a payment; members review and approve it
ome approve 4f2a                  # the review on your device, then your signature
ome execute 4f2a                  # prove and send once enough approved
ome watch                         # proposals and activity, live
ome daemon install-service        # keep your keeper answering while no terminal is open
```

`ome` works beside `aztec-wallet` and uses its address aliases; it keeps no address book of its own. `ome --help`
lists every command.

## What each party holds

| Party             | Holds                                                                                         |
| ----------------- | --------------------------------------------------------------------------------------------- |
| A member          | their signing key (never leaves their device) and, through their keeper, one viewing share    |
| A member's keeper | one share, sealed on the member's device or their own server; answers reads with proofs       |
| A relay           | ciphertext and peer ids: it forwards keeper traffic and keeps sealed mailboxes                |
| The Aztec network | proofs, encrypted notes and nullifiers; not who signed, the thresholds or the members         |
| Ome Labs          | runs one relay for Testnet, so it sees which peer ids talk; never a share, a key or a balance |

Fees are never paid from the vault in public. Shares are never backed up or exported: a lost key or keeper is
replaced by a key swap or a membership change while enough remain, and `ome vault health` shows the margin
([recovery](architecture/recovery.md)).

## Status

Ome runs on Aztec 6.0.0's Testnet and on a local network; mainnet is refused. It has not had an external audit yet:
use Testnet funds.

Working end to end: founding a vault through a lobby and a distributed key generation; paying with `k` approvals,
rejecting and cancelling; threshold reads; adding and removing members with a reshare (and the contract refusing a
committee that does not hold the vault's viewing key); keepers on separate networks reaching each other through
relays and catching up from their mailboxes; a member's signing key on a phone; a keeper on a member's own server over
pinned TLS. Proving times are in [architecture/README.md](architecture/README.md#measurements).

Not built yet: desktop and mobile apps (the command line is the way in), a recovery keeper (a viewer without a
signing key), push notifications, and P-256 signer keys held by the Secure Enclave or StrongBox.

## What is published

| Artifact                                   | What it is                                                                        |
| ------------------------------------------ | --------------------------------------------------------------------------------- |
| `@ome-labs/ome`                            | the `ome`, `omed` and `ome-keeper` commands; pulls in the build for your platform |
| `@ome-labs/daemon`                         | `omed`, a member's daemon: wallet, sealed signer, keeper supervision, API         |
| `@ome-labs/wallet`                         | Aztec's PXE host for Ome vaults: the threshold key store, deployment, proposals   |
| `@ome-labs/pxe`                            | Aztec's `@aztec-labs/pxe` 6.0.0 with the key-store hook Ome needs                 |
| `@ome-labs/api`                            | TypeScript types of the keeper and daemon APIs                                    |
| `@ome-labs/wasm`                           | the Rust core (cryptography, wire, protocol, signer) as WebAssembly               |
| `@ome-labs/contracts`                      | the `OmeVault` contract artifacts                                                 |
| `ghcr.io/ome-labs/ome-keeper`, `ome-relay` | a keeper for your own server, and a relay; amd64 and arm64, signed with cosign    |
| `ome-<version>-<platform>.tar.gz`          | the release archives the install script and Homebrew use, with SBOMs              |

## Repository

```text
contracts/      Aztec.nr 6.0.0: the OmeVault account contract and its tests
crates/         Rust: crypto, wire, protocol, signer, net, keeper, relay, api, client, cli, wasm, ffi
packages/       TypeScript: ome (launcher), daemon, wallet, pxe (fork), api
tools/          golden vectors from Aztec's own packages, and the multi-network suite
scripts/        repository scripts, the install script and release packaging
architecture/   one page per component: what is built, decided and measured
```

[CONTRIBUTING.md](CONTRIBUTING.md) covers the toolchain, the commands and how changes are made and tested.

## License

Copyright 2026 Vedant Chainani. Licensed under the [Apache License, Version 2.0](LICENSE).
