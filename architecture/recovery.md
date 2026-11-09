# Lost keys and lost shares

What a vault survives, what it does not, and how members recover. The short version: **a vault needs `k` signing keys
to spend and `t_v` keepers to read, and it cannot spend what it cannot read.** Keep a spare of each.

## Why reading matters for spending

A vault's funds are private notes. Its members' wallets find and decrypt them through threshold ECDH, which needs
`t_v` keepers to answer. A wallet that cannot read the notes cannot build a transaction that spends them, so a vault
that loses its viewing committee also loses its funds, even with every signing key intact.

## What is never done

Viewing shares are never backed up, exported, or sent anywhere except as protocol sub-shares during a reshare (a
product invariant, `CLAUDE.md`). A backup would be a copy of the secret that removal cannot reach: a member removed
later could restore it. So the only way back from a lost share is a reshare among the keepers that still hold one.

## What each loss means

| Lost                                        | While                      | Recovery                                                                                                                                                                                                                                                                 |
| ------------------------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| One member's signing key (device lost)      | `k` keys remain            | A **signer swap** (`ome member swap-key`): `k` members approve replacing that slot's key with the member's new device's. No reshare.                                                                                                                                     |
| One member's keeper (disk lost, share gone) | `t_v` keepers still answer | A **membership change** that removes the member and adds them back with a new keeper: the member runs `ome member request` on the new device, and a member runs `ome member change --remove <member> --add <request>`. The staying keepers reshare to the new committee. |
| A member entirely                           | both of the above          | The same membership change: it replaces the key and the share together.                                                                                                                                                                                                  |
| Signing keys below `k`                      | —                          | Nothing: no transaction can be authorized, including a rotation. The funds are stuck.                                                                                                                                                                                    |
| Keepers below `t_v`                         | —                          | Nothing: the vault can no longer find its notes, and no reshare can run (it needs `t_v` dealers). The funds are stuck.                                                                                                                                                   |

A rotation removes at most `t_v − 1` members at once, so a replacement that also removes members may need two steps.

## Choosing thresholds

`ome-protocol`'s policy warnings (`membership::policy::warnings`) name the shapes to avoid, and `ome` shows them when a
vault is created or its members change. The two that can strand funds need a deliberate yes (`--yes` in scripts):

- **`t_v = n`** (`NoSpareViewer`): losing any one keeper makes the vault unreadable for good. Every 2-member vault is
  like this, since `t_v ≥ 2`.
- **`k = n`** (`NoSpareSigner`): losing any one signing key stops all spending for good.

The others are warnings:

- **`k = 1`**: any one member can spend alone.
- **`t_v = 2` with 3 or more members**: any two members, including two who later leave, can read what arrived while
  they were members; removal then relies on removed members having erased their shares. `t_v ≥ 3` is safer.

A sensible default is 3 members with `k = 2` and `t_v = 2`, or 5 members with `k = 3` and `t_v = 3`: one spare of
each.

## Watching for it

`ome vault health` says how many keepers answer now, how many more can be lost, whether this member's keeper holds its
share and the on-chain config names its committee, and the thresholds' risks. It exits 4 when the vault cannot read,
so it can run from a monitor. `--json` gives `answering`, `spare`, `canRead` and the risks.

## Not built

- A recovery keeper: a committee member that holds a share but no signing key (a member's own server, or a trusted
  party), to keep a spare viewer without another signer. The contract ties committee member `i` to signer `i` today.
- Social recovery of a single member's whole profile.
