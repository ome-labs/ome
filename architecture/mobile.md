# The mobile signer

A member can move their signing key to a phone. The phone reviews and signs proposals; the member's keeper and
daemon keep running on a desktop or on the member's own server. Nothing changes for the other members: their keepers
never learn that a phone exists.

## Roles

| Role                   | Holds                                                                                                      | Runs on                                               |
| ---------------------- | ---------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Keeper (`ome-keeper`)  | the viewing share; threshold ECDH; DKG and resharing; replicated records; **the phone's pairing**          | an always-on machine: home server, VPS, or the laptop |
| Daemon (`omed`)        | the PXE (the vault's decrypted notes), vault records, epoch secrets; builds, proves and sends transactions | beside the keeper                                     |
| Phone (`ome-ffi` core) | **the member's signing key**, its device keys, its pairing and its local policy                            | the member's phone                                    |

The phone never holds a viewing share, a PXE or any vault protocol secret. It never proves or sends a transaction.

### Setups

```text
desktop only     laptop: ome CLI → omed (PXE, signer) → ome-keeper
laptop + phone   laptop: omed + ome-keeper   ⇄ relay mailbox ⇄   phone (signing key)
server + phone   server: omed + ome-keeper   ⇄ relay mailbox ⇄   phone (signing key)
```

Server and phone is the recommended mobile setup: the keeper stays online for the other members' threshold reads.

### What a stolen device gives away

| Lost or stolen | Gives                                                        | Does not give                                         |
| -------------- | ------------------------------------------------------------ | ----------------------------------------------------- |
| the server     | what the member can already see; one viewing share (< `t_v`) | the full viewing secret; any signature                |
| the phone      | one signing key (< `k`), behind the platform's user check    | anything readable: no share, no PXE, no vault secrets |
| a relay        | ciphertext, sizes, timing                                    | contents, vault addresses, signer keys                |

## Why the keeper is the phone's endpoint

The keeper already runs the member's libp2p node, polls the relay mailboxes, and verifies, stores and replicates
votes. Making it the phone's counterpart adds no second network node and no new trust: a vote from the phone is
checked exactly like a vote from the daemon (the Schnorr signature of the key in its slot) and replicated the same
way. The daemon decides what to ask the phone, because only it reads the chain: it sends each proposal for review with
the vault's current config and its own review.

## Device keys

A phone has three keys, all generated on the phone:

| Key         | Type               | Used for                                                | Unlocked by               |
| ----------- | ------------------ | ------------------------------------------------------- | ------------------------- |
| identity    | Ed25519            | signing every device frame; the phone's libp2p peer id  | the transport sealing key |
| transport   | X25519             | mailbox ids and HPKE envelopes with the member's keeper | the transport sealing key |
| signing key | Grumpkin (Schnorr) | approvals and rejections, counted by the vault          | the signing sealing key   |

- The Secure Enclave and StrongBox hold only P-256 keys, so the Grumpkin key is sealed instead: the app keeps two
  32-byte **sealing keys** in the Keychain (iOS) or behind the Android Keystore, and hands them to the core through the
  `DeviceStore` callback. The signing sealing key must require the user (biometrics or device passcode); the transport
  one may open without them, so the app can check its mailbox in the background.
- The signing key is sealed with `ome-signer`'s `SealedKey` (`kdf: external`) and opened for one signature only.
  The identity and transport secrets are sealed together with ChaCha20-Poly1305 under the transport sealing key
  (associated data `ome.device.keys.v0`).
- Later (v1): signer keys that live in the chip, if the contract learns to verify P-256 signatures. That changes the
  contract, the witness layout and the gate counts.

## Pairing

1. The member's daemon asks its keeper for an offer (`devices.offer`). The keeper draws a one-time 32-byte secret,
   keeps it in memory for 10 minutes, and returns the offer string, shown as a QR code:

   ```text
   ome-pair-v0:<base64url(canonical(PairOffer))>
   PairOffer { offer id (16 B), keeper identity, keeper transport key, relays, secret (32 B), expires_at,
               member name, network label }
   ```

   The offer holds a secret: whoever scans it can pair until it expires or is used.

2. The phone sends `DevicePair { offer, name, transport, signer_key }`, signed by its identity, into the **pairing
   mailbox** at the offer's relays:
   - mailbox id `SHA-256("ome.pair.mailbox.v0" ‖ secret)`;
   - sealed with XChaCha20-Poly1305 under `HMAC-SHA256(secret, "ome.pair.key.v0")`, associated data the mailbox id.

   Only someone holding the offer can write a pairing the keeper opens, and the relay sees ciphertext under an id it
   cannot link to anything.

3. The keeper polls the pairing mailbox of each open offer with its other mailboxes. It accepts the first valid
   `DevicePair`, consumes the offer, stores the device (identity, transport key, signer key, name) and answers with
   `DevicePaired { offer, member, network, fee_payers }` through the ordinary mailbox between the two transport keys.
   The fee payers are the ones the member's own daemon trusts; the phone keeps them for rotation approvals.

4. `devices.revoke` deletes the device and sends it `DeviceUnpair`. The keeper ignores a revoked device's frames.

## Messages

Device messages are ordinary `ome-wire` signed messages (new `Message` variants, v0):

| Message        | From → to      | Body                                                                                                    |
| -------------- | -------------- | ------------------------------------------------------------------------------------------------------- |
| `DevicePair`   | phone → keeper | offer id, device name, transport key, signer key                                                        |
| `DevicePaired` | keeper → phone | offer id, member name, network label, trusted fee payers                                                |
| `DeviceReview` | keeper → phone | vault, vault name, manifest, config, chain, proposal, payload, labels, simulation, an optional rotation |
| `DeviceVote`   | phone → keeper | vault and an approval or a rejection                                                                    |
| `DeviceVoted`  | keeper → phone | proposal, and why the vote was refused, if it was                                                       |
| `DeviceUnpair` | either way     | reason                                                                                                  |

- **Transport**: the relay mailbox (`ome-net` `RelayMailbox`, libp2p) only. The phone runs a short-lived client node
  that dials the relays, deposits or fetches, and stops; it listens on nothing and reserves no circuit. Envelopes
  live for the keeper's mailbox TTL (7 days).
- **Freshness**: device frames are idempotent and verified on their own, and may wait in a mailbox for days while one
  side is offline, so they skip the keeper's replay window, like replicated records. A vote is checked against its
  proposal; a pairing against its offer's expiry; a review against its proposal's deadline.
- **Binding**: the keeper accepts a device frame only on the mailbox channel of a paired device's transport key and
  only when the frame's signer is that device's identity.
- **Size**: the largest review is a 256 KiB payload plus a full manifest, about 290 KiB, under the relay's 512 KiB
  envelope limit.
- **Notifications**: the app checks the mailbox when opened and on background refresh. Push (APNs, FCM) is out of
  scope: it needs a credentialed sender, close to a central server. If added, it would carry content-free wake-ups,
  opt in per member.

## Reviewing on the phone

The phone **verifies itself**:

- the manifest against the config (`Vault::from_manifest`): signer set, thresholds and committee agree;
- the proposal (`check_proposal`): the payload is the one referenced, the calls hash to the outer hash on this vault
  and chain, the config version is current, the deadline has not passed, fees stay off the vault;
- every call's function signature against its selector, and the arguments decoded by the signature's types
  (`payload::arguments`);
- that the phone's signing key holds a slot in the vault;
- its local policy: contracts it allows, per-token limits (the first `u128` argument of a call to that token), and
  whether every contract must carry a label pinned on the phone;
- for a rotation: the reshared committee holds the vault's key and its shares are fresh (`check_rotation`); for a
  signer swap: only one key changes (`check_signer_swap`).

The phone **trusts the member's own keeper and daemon** for:

- that the config is the current one (only a PXE reads the config note);
- labels it has not pinned, shown as claimed;
- the simulation result;
- for a rotation, that it is the one this member's keeper reshared.

A lying keeper or daemon can make the phone refuse a valid proposal or show a wrong claimed label. It cannot make the
phone sign calls other than those shown, because the phone computes what it signs from the calls themselves.

## Enrolling a phone: the signer swap

A phone's key replaces the member's key in the member's slot.

- **On chain**: `rotate_membership` with the committee shares, `k` and `t_v` unchanged and one signer key replaced.
  The contract already accepts it: `governance::rotate` checks the current set, the thresholds, a well-formed
  committee, and fewer than `t_v` removals, and a swap removes one key.
- **No reshare**: the committee and its epoch stay, so keepers keep their shares and no ceremony runs.
- **Approving**: members approve it with `approve_signer_swap`, which allows only the swap and calls to the member's
  trusted fee payers. Any `k` members could already replace a member through a rotation, so a swap gives them nothing
  new; the member whose key changes usually proposes it.
- **Keepers**: every keeper verifies votes against the manifest's signer keys, so each must adopt the new key.
  `vault.updateSigners(vault, proposal)` derives the new manifest from the executed proposal's own `rotate_membership`
  call (`swapped_manifest`) and adopts it only if it passes `Vault::from_manifest` against the latest config the
  daemon observed. Each daemon calls it when the observed config's signer set no longer matches its manifest.
- **After the swap**: the member's daemon sees that its slot holds a paired device's key and forwards proposals to
  it; its own approvals are refused with "approve on your phone". Losing the phone is the same flow, proposed by any
  member with the new device's key.

## The binding (`ome-ffi`)

`crates/ffi` exposes the phone core through UniFFI 0.32 (Swift and Kotlin). It depends on `ome-crypto`, `ome-wire`,
`ome-protocol`, `ome-signer` and `ome-net`, never on the keeper or the threshold code.

| Object or function | What it does                                                                                                          |
| ------------------ | --------------------------------------------------------------------------------------------------------------------- |
| `DeviceStore`      | implemented by the app: `read`, `write`, `remove` for the core's files, `sealing_key` for the two sealing keys        |
| `OmeDevice`        | `create`, `open`, `info`, `pair`, `pairing`, `sync`, `reviews`, `approve`, `reject`, `policy`, `set_policy`, `unpair` |
| `parse_pair_offer` | reads an offer before pairing, to show whose keeper it is                                                             |

See [bindings.md](bindings.md) for the build, packaging and tests.
