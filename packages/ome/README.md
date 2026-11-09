# @ome-labs/ome

The [Ome](https://ome.network) command line for members of a private k-of-n multisig vault on Aztec. This is the
package to install if you want to use Ome.

It installs three programs:

- `ome`, the command line;
- `omed`, the per-profile daemon `ome` starts for you (the wallet, your sealed signing key, your keeper);
- `ome-keeper`, which holds your share of each vault's viewing key.

`ome` and `ome-keeper` are native: npm installs the build for your platform from `@ome-labs/ome-darwin-arm64`,
`@ome-labs/ome-linux-x64` or `@ome-labs/ome-linux-arm64`. Other platforms are not supported yet.

## Install

```sh
npm install --global @ome-labs/ome    # Node 24 or later
```

The same programs are available with Homebrew (`brew install ome-labs/tap/ome`) and the install script
(`curl -fsSL https://ome.network/install.sh | sh`).

## Use

```sh
ome init                          # your profile: name, network, passphrase
ome vault create                  # found a vault with co-founders: prints the invite
ome vault join <invite>           # each co-founder
ome propose transfer 250 USDC bob # a payment the members review
ome approve 4f2a                  # review on your device, then sign
ome execute 4f2a                  # prove and send once enough approved
ome doctor                        # check the installation
```

`ome --help` lists every command. Ome works with Aztec's Testnet and a local network.

## Documentation

- [The command line](https://github.com/ome-labs/ome/blob/main/architecture/cli.md)
- [The daemon](https://github.com/ome-labs/ome/blob/main/architecture/daemon.md) and [the keeper](https://github.com/ome-labs/ome/blob/main/architecture/keeper.md)
- [Losing a key or a keeper](https://github.com/ome-labs/ome/blob/main/architecture/recovery.md)

## Versions

`@ome-labs/ome` and the other `@ome-labs` packages are released together with one version. This release works with
Aztec 6.0.0.

## License

Apache-2.0
