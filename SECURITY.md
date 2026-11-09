# Security

Ome holds shared funds and keeps them private, so reports about either are the most valuable ones we get.

## Reporting a vulnerability

Report privately through GitHub: [Security → Report a vulnerability](https://github.com/ome-labs/ome/security/advisories/new).
Please do not open a public issue, pull request or discussion for a vulnerability.

Include what you found, how to reproduce it (the version, `ome doctor --json`, the commands or a test), and what an
attacker gains. We reply within three working days, keep you informed while we fix it, and credit you in the advisory
unless you prefer otherwise. Please give us 90 days, or until a fix is released if that is sooner, before disclosing.

Never include a real signing key, viewing share, passphrase or a vault's sealed files in a report.

## Scope

In scope: the `OmeVault` contract, the Rust crates, `@ome-labs/*` packages, the `ome`, `omed`, `ome-keeper` and
`ome-relay` programs, the container images, the install script and the release pipeline. Of most interest:

- spending without `k` valid signatures, or with signatures for a different payload;
- reading a vault with fewer than `t` keepers, or by a member after their removal;
- anything that writes signers, thresholds or the committee to public storage or public arguments, or pays fees
  from the vault in public;
- a signing key, viewing share or passphrase leaving the device it belongs to, or reaching logs;
- a keeper accepting a forged partial, an unauthenticated RPC call or a replayed message;
- a relay or network observer learning more than ciphertext and peer ids;
- tampering with a release that `architecture/release.md`'s checks would not catch.

Out of scope: vulnerabilities in Aztec itself (report them to Aztec), denial of service by a relay you chose to use
(members can run their own), and attacks that require an unlocked device you control.

## Supported versions

Only the latest release receives fixes. Ome has not had an external audit yet; use Testnet funds.

## Verifying what you run

Every release is built by `.github/workflows/release.yml` in public, with GitHub build attestations for the
archives, npm provenance for the packages and cosign signatures for the images.
[architecture/release.md](architecture/release.md) shows how to check each.
