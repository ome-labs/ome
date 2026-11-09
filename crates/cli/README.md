# ome-cli

`ome`, an Ome member's command line. It is a client of the member's daemon (`omed`), which it starts per profile, and
does vault work only: ordinary accounts, deployments and Fee Juice stay with `aztec-wallet`, whose aliases it reads.

```sh
ome init                          # profile, passphrase, network; starts the daemon
ome vault create                  # found a vault with co-founders
ome propose transfer 250 USDC bob # a payment the members review
ome approve 4f2a                  # review on your device, then sign
ome execute 4f2a                  # prove and send
ome doctor                        # how Ome is installed, and what is missing
```

Every command takes `--json`, `--plain` and `--no-input` for scripts. Passphrases are never read from arguments:
use the prompt, Touch ID (`ome signer touch-id on`) or `OME_PASSPHRASE_FILE`.

## Install

`npm install --global @ome-labs/ome`, `brew install ome-labs/tap/ome`, or `curl -fsSL https://ome.network/install.sh | sh`.
From a checkout, `pnpm cli:install` builds `ome`, `ome-keeper` and an `omed` wrapper into `~/.ome/bin`.

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-cli
```

## Learn more

[architecture/cli.md](https://github.com/ome-labs/ome/blob/main/architecture/cli.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
