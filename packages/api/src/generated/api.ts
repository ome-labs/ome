// Generated from crates/api by `pnpm --filter @ome-labs/api generate`. Do not edit.

/**
 * 32 bytes: addresses, field elements, hashes, keys, session ids.
 */
export type Hex32 = string;

/**
 * 64 bytes: ceremony transcript hashes.
 */
export type Hex64 = string;

/**
 * 16 bytes: lobby ids.
 */
export type Hex16 = string;

/**
 * Bytes of any length: canonical wire encodings of proposals, payloads and votes.
 */
export type HexBytes = string;

/**
 * A Grumpkin point as its affine coordinates. Whether it is on the curve is checked where it is used.
 */
export type Point = {
  /**
   * `x`.
   */
  x: Hex32;
  /**
   * `y`.
   */
  y: Hex32;
};

/**
 * A token amount in base units (`u128`), written as a decimal string.
 */
export type Amount = string;

/**
 * Params of a method that takes none. Sent as `{}`; servers also accept a missing or `null` params member.
 */
export type Empty = Record<string, never>;

/**
 * Params of `session.auth`, the first call of every session.
 */
export type AuthParams = {
  /**
   * The per-launch token the service wrote next to its socket.
   */
  token: string;
};

/**
 * What went wrong, in terms a client can act on. The CLI maps kinds to exit codes.
 */
export type ErrorKind =
  | "parse"
  | "invalid-request"
  | "method-not-found"
  | "invalid"
  | "unauthorized"
  | "not-found"
  | "not-ready"
  | "locked"
  | "auth"
  | "alarm"
  | "failed";

/**
 * The `data` member of an error response.
 */
export type ErrorData = {
  /**
   * What went wrong.
   */
  kind: ErrorKind;
};

/**
 * A JSON-RPC error object.
 */
export type RpcError = {
  /**
   * JSON-RPC code, [`ErrorKind::code`].
   */
  code: number;
  /**
   * Human-readable, never containing a secret.
   */
  message: string;
  /**
   * The kind; absent only from a server that is not an Ome service.
   */
  data?: ErrorData;
};

/**
 * What `keeper.info` reports.
 */
export type KeeperInfo = {
  /**
   * The API version this keeper serves, [`crate::API_VERSION`].
   */
  apiVersion: number;
  /**
   * Keeper release.
   */
  version: string;
  /**
   * Keeper identity: its Ed25519 public key, also its libp2p peer id.
   */
  identity: Hex32;
  /**
   * X25519 key mailbox envelopes to this keeper are sealed to.
   */
  transport: Hex32;
  /**
   * libp2p peer id.
   */
  peerId: string;
  /**
   * Addresses the keeper advertises.
   */
  addresses: Array<string>;
};

/**
 * How to reach a keeper.
 */
export type KeeperContact = {
  /**
   * Keeper identity.
   */
  identity: Hex32;
  /**
   * Keeper transport key.
   */
  transport: Hex32;
  /**
   * Advertised addresses.
   */
  addresses: Array<string>;
};

/**
 * A committee member.
 */
export type Member = {
  /**
   * 1-based index.
   */
  index: number;
  /**
   * Approval signing key.
   */
  signerKey: Point;
  /**
   * Public verification share of the member's viewing share.
   */
  verificationShare: Point;
  /**
   * Keeper identity.
   */
  identity: Hex32;
  /**
   * Keeper transport key.
   */
  transport: Hex32;
  /**
   * Keeper addresses.
   */
  addresses: Array<string>;
};

/**
 * A committee manifest: one vault epoch's members, thresholds and group key.
 */
export type Manifest = {
  /**
   * Protocol version the members agreed on.
   */
  protocolVersion: number;
  /**
   * Vault address.
   */
  vault: Hex32;
  /**
   * Committee epoch.
   */
  epoch: number;
  /**
   * Spending threshold `k`.
   */
  threshold: number;
  /**
   * Viewing threshold `t_v`.
   */
  viewingThreshold: number;
  /**
   * Preaddress.
   */
  preaddress: Hex32;
  /**
   * Group key `Ivpk`.
   */
  groupKey: Point;
  /**
   * Members in slot order.
   */
  members: Array<Member>;
};

/**
 * Whether the keeper still serves a vault.
 */
export type VaultStatus = "active" | "retired" | "removed";

/**
 * A vault the keeper knows.
 */
export type Vault = {
  /**
   * Current manifest.
   */
  manifest: Manifest;
  /**
   * Status.
   */
  status: VaultStatus;
  /**
   * This keeper's member index, if a member.
   */
  index: number | null;
  /**
   * Whether the keeper holds its share for the current epoch.
   */
  serving: boolean;
};

/**
 * The vault's config note, as the daemon read it from the chain.
 */
export type VaultConfig = {
  /**
   * Spending threshold `k`.
   */
  threshold: number;
  /**
   * Registered signers.
   */
  signerCount: number;
  /**
   * Config version.
   */
  version: number;
  /**
   * Commitment to the signer set.
   */
  signerSetCommitment: Hex32;
  /**
   * Viewing threshold `t_v`.
   */
  viewingThreshold: number;
  /**
   * Commitment to the viewing committee.
   */
  committeeCommitment: Hex32;
};

/**
 * What an observed config meant to the keeper.
 */
export type Observation = "current" | "outdated" | "activated" | "removed" | "unknown";

/**
 * The result of `vault.observeConfig`.
 */
export type Observed = {
  /**
   * What the config meant.
   */
  observation: Observation;
};

/**
 * How reachable a committee member is.
 */
export type Reachability = {
  /**
   * Member index.
   */
  index: number;
  /**
   * Keeper identity.
   */
  identity: Hex32;
  /**
   * libp2p peer id.
   */
  peerId: string;
  /**
   * Whether the member is this keeper.
   */
  you: boolean;
  /**
   * There is an established connection.
   */
  connected: boolean;
  /**
   * Every connection is a relay circuit.
   */
  relayed: boolean;
  /**
   * Unix seconds of the last connection or frame.
   */
  lastSeen: number | null;
  /**
   * Last round trip in milliseconds.
   */
  rttMs: number | null;
};

/**
 * One served or refused ECDH request.
 */
export type AuditEntry = {
  /**
   * Unix seconds.
   */
  at: number;
  /**
   * `local`, or the requesting keeper's identity.
   */
  requester: string;
  /**
   * What the points were for.
   */
  purpose: string;
  /**
   * Number of points.
   */
  points: number;
  /**
   * `served`, or why it was refused.
   */
  outcome: string;
};

/**
 * Params naming a vault.
 */
export type VaultParams = {
  /**
   * Vault address.
   */
  vault: Hex32;
};

/**
 * Params naming a ceremony.
 */
export type SessionParams = {
  /**
   * Session id.
   */
  session: Hex32;
};

/**
 * Params of `vault.register`.
 */
export type RegisterParams = {
  /**
   * The founding DKG's session.
   */
  session: Hex32;
  /**
   * The deployed vault's manifest.
   */
  manifest: Manifest;
};

/**
 * Params of `vault.observeConfig`.
 */
export type ObserveConfigParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Its config note as read from the chain.
   */
  config: VaultConfig;
};

/**
 * Params of `vault.retire`.
 */
export type RetireParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Retire even though this keeper has not seen a newer config: for a member removed by a rotation its keeper took
   * no part in, which cannot read the new config.
   */
  force?: boolean;
};

/**
 * Params of `audit.list`.
 */
export type AuditParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Most recent entries to return; 100 by default, at most 10,000.
   */
  limit?: number;
};

/**
 * A member of a reshare's new committee.
 */
export type NewMember = {
  /**
   * Keeper identity.
   */
  identity: Hex32;
  /**
   * Keeper transport key.
   */
  transport: Hex32;
  /**
   * Keeper addresses.
   */
  addresses: Array<string>;
  /**
   * Approval signing key.
   */
  signerKey: Point;
};

/**
 * Which ceremony.
 */
export type CeremonyKind = "dkg" | "reshare";

/**
 * Where a ceremony is.
 */
export type CeremonyStatus = "running" | "completed" | "aborted" | "activated" | "expired";

/**
 * Why a ceremony stopped.
 */
export type Failure = {
  /**
   * Reason.
   */
  reason: string;
  /**
   * Identities of the participants held responsible, if any.
   */
  blamed: Array<string>;
};

/**
 * A finished DKG.
 */
export type DkgResult = {
  /**
   * This keeper's member index.
   */
  index: number;
  /**
   * Viewing threshold.
   */
  threshold: number;
  /**
   * Group key `Ivpk`.
   */
  groupKey: Point;
  /**
   * Verification shares in member order.
   */
  verificationShares: Array<Point>;
  /**
   * Transcript hash every member must agree on.
   */
  transcript: Hex64;
};

/**
 * A finished reshare.
 */
export type ReshareResult = {
  /**
   * The new committee's commitment, for `rotate_membership`.
   */
  committeeCommitment: Hex32;
  /**
   * This keeper's index in the new committee, if it stays.
   */
  newIndex: number | null;
  /**
   * The new manifest, if this keeper stays.
   */
  manifest: Manifest | null;
  /**
   * Transcript hash, if this keeper stays.
   */
  transcript: Hex64 | null;
  /**
   * For a member who joined in this reshare: the vault's shared protocol secrets a dealer handed it.
   */
  secrets: HexBytes | null;
};

/**
 * A ceremony's outcome.
 */
export type CeremonyResult = DkgResult | ReshareResult;

/**
 * A ceremony as `ceremony.status` reports it.
 */
export type Ceremony = {
  /**
   * Which ceremony.
   */
  kind: CeremonyKind;
  /**
   * Where it is.
   */
  status: CeremonyStatus;
  /**
   * Why it stopped, if aborted.
   */
  failure: Failure | null;
  /**
   * Its outcome, once completed.
   */
  result: CeremonyResult | null;
};

/**
 * Params of `ceremony.dkg`. Every founding member's daemon calls it with the same values.
 */
export type DkgParams = {
  /**
   * Session id.
   */
  session: Hex32;
  /**
   * Viewing threshold.
   */
  threshold: number;
  /**
   * Every member's keeper, in member order.
   */
  members: Array<KeeperContact>;
};

/**
 * Params of `ceremony.reshare`. Every dealer's and new member's daemon calls it with the same values.
 */
export type ReshareParams = {
  /**
   * Session id.
   */
  session: Hex32;
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * The current manifest: required from a joining member's keeper, which does not know the vault.
   */
  current: Manifest | null;
  /**
   * Dealing members' indices.
   */
  dealers: Array<number>;
  /**
   * New viewing threshold.
   */
  newThreshold: number;
  /**
   * New spending threshold.
   */
  newSpendingThreshold: number;
  /**
   * The new committee, in member order.
   */
  newMembers: Array<NewMember>;
  /**
   * From a dealer's daemon: the vault's shared protocol secrets, which the keeper seals to every joining member.
   * Never stored by the dealer's keeper.
   */
  secrets?: HexBytes;
};

/**
 * What an ECDH batch is for. Recorded in the audit log.
 */
export type EcdhPurpose = "decryption" | "tagging" | "sender-tag" | "handshake";

/**
 * Params of `ecdh.batch`.
 */
export type EcdhParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Committee epoch the PXE expects.
   */
  epoch: number;
  /**
   * What the points are for.
   */
  purpose: EcdhPurpose;
  /**
   * Ephemeral points.
   */
  points: Array<Point>;
};

/**
 * One partial `D_i = s_i · P` with its DLEQ proof.
 */
export type Partial = {
  /**
   * `D_i`.
   */
  point: Point;
  /**
   * DLEQ challenge.
   */
  challenge: Hex32;
  /**
   * DLEQ response.
   */
  response: Hex32;
};

/**
 * One member's partials, in input order.
 */
export type MemberPartials = {
  /**
   * Member index.
   */
  member: number;
  /**
   * Partials.
   */
  partials: Array<Partial>;
};

/**
 * The result of `ecdh.batch`: at least `t_v` members' verified partials.
 */
export type EcdhResult = {
  /**
   * Committee epoch.
   */
  epoch: number;
  /**
   * Verified partials, this keeper's included.
   */
  responses: Array<MemberPartials>;
};

/**
 * Params of `proposals.put`, `payloads.put`, `approvals.put` and `rejections.put`.
 */
export type BodyParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Canonical encoding.
   */
  body: HexBytes;
};

/**
 * The result of `proposals.put`.
 */
export type ProposalStored = {
  /**
   * Proposal id.
   */
  id: Hex32;
};

/**
 * The result of `payloads.put`.
 */
export type PayloadStored = {
  /**
   * Payload hash, as the proposal references it.
   */
  hash: Hex32;
};

/**
 * Params naming one proposal.
 */
export type IdParams = {
  /**
   * Proposal id.
   */
  id: Hex32;
};

/**
 * Params naming a proposal whose votes are listed.
 */
export type ProposalParams = {
  /**
   * Proposal id.
   */
  proposal: Hex32;
};

/**
 * Params of `payloads.get`.
 */
export type PayloadParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Payload hash.
   */
  hash: Hex32;
};

/**
 * Params of `receipts.put`.
 */
export type ReceiptParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Executed proposal.
   */
  proposal: Hex32;
  /**
   * Transaction that executed it.
   */
  txHash: Hex32;
};

/**
 * An execution receipt.
 */
export type Receipt = {
  /**
   * Executed proposal.
   */
  proposal: Hex32;
  /**
   * Transaction that executed it.
   */
  txHash: Hex32;
  /**
   * Unix seconds it was stored.
   */
  at: number;
};

/**
 * Params of `vault.observeTx`.
 */
export type TxParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * A transaction the vault sent.
   */
  txHash: Hex32;
};

/**
 * The result of `vault.observeTx`.
 */
export type TxObserved = {
  /**
   * Whether a receipt names it; if not, the keeper raised `unexpectedSpend`.
   */
  known: boolean;
};

/**
 * The result of `mailbox.poll`.
 */
export type MailboxPolled = {
  /**
   * Envelopes delivered.
   */
  delivered: number;
};

/**
 * The member a keeper represents in a lobby.
 */
export type LobbyMemberParams = {
  /**
   * Display name.
   */
  name: string;
  /**
   * Approval signing key.
   */
  signerKey: Point;
};

/**
 * Params of `lobby.open`.
 */
export type LobbyOpenParams = {
  /**
   * Vault name.
   */
  name: string;
  /**
   * Proposed spending threshold.
   */
  threshold: number;
  /**
   * Proposed viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Network the vault will live on, as the daemon names it.
   */
  network: string;
  /**
   * The host member.
   */
  member: LobbyMemberParams;
};

/**
 * The result of `lobby.open`.
 */
export type LobbyOpened = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * Invite to share with the other members.
   */
  invite: string;
};

/**
 * Params of `lobby.join`.
 */
export type LobbyJoinParams = {
  /**
   * Invite from the host.
   */
  invite: string;
  /**
   * The joining member.
   */
  member: LobbyMemberParams;
};

/**
 * The result of `lobby.join`.
 */
export type LobbyJoined = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
};

/**
 * Params of `lobby.inspect`.
 */
export type InviteParams = {
  /**
   * Invite.
   */
  invite: string;
};

/**
 * What an invite says, decoded without any network traffic.
 */
export type Invite = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * Vault name.
   */
  name: string;
  /**
   * Host's display name.
   */
  hostName: string;
  /**
   * Proposed spending threshold.
   */
  threshold: number;
  /**
   * Proposed viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Network.
   */
  network: string;
  /**
   * The host's keeper.
   */
  host: KeeperContact;
};

/**
 * A join request for an existing vault, as `keeper.joinRequest` and `membership.request` return it.
 */
export type JoinRequestText = {
  /**
   * `ome-join-v0:…`, signed by the newcomer's keeper; passed out of band to a member.
   */
  request: string;
};

/**
 * Params naming a lobby.
 */
export type LobbyParams = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
};

/**
 * Params of `lobby.start`.
 */
export type LobbyStartParams = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * Final spending threshold.
   */
  threshold: number;
  /**
   * Final viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Deployment salt.
   */
  salt: Hex32;
  /**
   * The host's sealed founding secrets, handed to every guest with the plan.
   */
  secrets: HexBytes;
};

/**
 * This keeper's part in a lobby.
 */
export type LobbyRole = "host" | "guest";

/**
 * Where a lobby is.
 */
export type LobbyStatus = "open" | "started" | "closed";

/**
 * A member on a lobby's roster.
 */
export type LobbyMember = {
  /**
   * 1-based position, the future member index.
   */
  index: number;
  /**
   * Display name.
   */
  name: string;
  /**
   * Approval signing key.
   */
  signerKey: Point;
  /**
   * Keeper identity.
   */
  identity: Hex32;
  /**
   * Keeper transport key.
   */
  transport: Hex32;
  /**
   * Keeper addresses.
   */
  addresses: Array<string>;
  /**
   * Whether the member is this keeper's.
   */
  you: boolean;
};

/**
 * The plan a started lobby fixed.
 */
export type LobbyPlan = {
  /**
   * DKG session id.
   */
  session: Hex32;
  /**
   * Spending threshold.
   */
  threshold: number;
  /**
   * Viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Deployment salt.
   */
  salt: Hex32;
  /**
   * The host's sealed founding secrets, until the founding finishes.
   */
  secrets: HexBytes | null;
  /**
   * Protocol version the vault will speak.
   */
  protocolVersion: number;
};

/**
 * A lobby.
 */
export type Lobby = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * This keeper's part.
   */
  role: LobbyRole;
  /**
   * Where it is.
   */
  status: LobbyStatus;
  /**
   * Vault name.
   */
  name: string;
  /**
   * Host's display name.
   */
  hostName: string;
  /**
   * Proposed spending threshold.
   */
  threshold: number;
  /**
   * Proposed viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Network.
   */
  network: string;
  /**
   * The host's keeper.
   */
  host: KeeperContact;
  /**
   * Roster in join order.
   */
  members: Array<LobbyMember>;
  /**
   * The plan, once started.
   */
  plan: LobbyPlan | null;
  /**
   * The invite, on the host.
   */
  invite: string | null;
};

/**
 * Params of `devices.offer`: what the pairing offer tells the device about its member.
 */
export type DeviceOfferParams = {
  /**
   * The member's display name.
   */
  member: string;
  /**
   * Network label, as in invites.
   */
  network: string;
  /**
   * Fee payer contracts the member's configuration trusts; the device allows a rotation to also call these.
   */
  feePayers: Array<Hex32>;
};

/**
 * A pairing offer: the `ome-pair-v0:` string to show as a QR code. It holds a one-time secret.
 */
export type DeviceOffer = {
  /**
   * The offer string.
   */
  offer: string;
  /**
   * Unix time after which the keeper refuses it.
   */
  expiresAt: number;
};

/**
 * A paired signing device.
 */
export type Device = {
  /**
   * The device's Ed25519 identity.
   */
  identity: Hex32;
  /**
   * Its X25519 transport key.
   */
  transport: Hex32;
  /**
   * The Grumpkin key it signs approvals with.
   */
  signerKey: Point;
  /**
   * Its display name.
   */
  name: string;
  /**
   * When it paired (unix seconds).
   */
  pairedAt: number;
};

/**
 * Params naming a device.
 */
export type DeviceParams = {
  /**
   * The device's identity.
   */
  identity: Hex32;
};

/**
 * The chain a vault lives on.
 */
export type Chain = {
  /**
   * L1 chain id, as a field.
   */
  chainId: Hex32;
  /**
   * Rollup version, as a field.
   */
  version: Hex32;
};

/**
 * A name the daemon knows for an address.
 */
export type DeviceLabel = {
  /**
   * Address.
   */
  address: Hex32;
  /**
   * Label.
   */
  label: string;
};

/**
 * The rotation this member's keeper reshared, sent with a `rotate_membership` proposal.
 */
export type DeviceRotation = {
  /**
   * New signer keys in slot order.
   */
  signers: Array<Point>;
  /**
   * New spending threshold.
   */
  threshold: number;
  /**
   * New viewing threshold.
   */
  viewingThreshold: number;
  /**
   * The reshared committee's verification shares.
   */
  verificationShares: Array<Point>;
};

/**
 * Params of `devices.review`: a stored proposal to send to the device that holds this member's signing key in the
 * vault, with what only the daemon knows.
 */
export type DeviceReviewParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Proposal id; the keeper must hold the proposal and its payload.
   */
  proposal: Hex32;
  /**
   * The vault's display name.
   */
  vaultName: string;
  /**
   * The config note as the daemon read it.
   */
  config: VaultConfig;
  /**
   * The vault's chain.
   */
  chain: Chain;
  /**
   * Names for the addresses involved.
   */
  labels: Array<DeviceLabel>;
  /**
   * The daemon's simulation outcome.
   */
  simulation?: string;
  /**
   * For a reshare rotation, the rotation this member's keeper reshared.
   */
  rotation?: DeviceRotation;
};

/**
 * The result of `devices.review`.
 */
export type DeviceReviewSent = {
  /**
   * Devices the review was sent to.
   */
  devices: number;
};

/**
 * Params of `vault.updateSigners`.
 */
export type UpdateSignersParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * The executed signer-swap proposal.
   */
  proposal: Hex32;
};

/**
 * Something the member's daemon should know about.
 */
export type KeeperEvent =
  | {
      type: "ceremony";
      /**
       * Session id.
       */
      session: Hex32;
      /**
       * Which ceremony.
       */
      kind: CeremonyKind;
      /**
       * Its new status.
       */
      status: CeremonyStatus;
    }
  | {
      type: "invalidPartials";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Member index.
       */
      member: number;
    }
  | {
      type: "committeeActivated";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Committee epoch.
       */
      epoch: number;
    }
  | {
      type: "removed";
      /**
       * Vault.
       */
      vault: Hex32;
    }
  | {
      type: "configMismatch";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Observed config version.
       */
      version: number;
      /**
       * Why the config was not accepted.
       */
      reason: string;
    }
  | {
      type: "unexpectedSpend";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Transaction hash.
       */
      txHash: Hex32;
    }
  | {
      type: "proposal";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      id: Hex32;
    }
  | {
      type: "payload";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Payload hash.
       */
      hash: Hex32;
    }
  | {
      type: "approval";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * Signer slot.
       */
      slot: number;
    }
  | {
      type: "rejection";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * Signer slot.
       */
      slot: number;
    }
  | {
      type: "receipt";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * Transaction hash.
       */
      txHash: Hex32;
    }
  | {
      type: "lobby";
      /**
       * Lobby id.
       */
      lobby: Hex16;
      /**
       * Its status.
       */
      status: LobbyStatus;
      /**
       * Members on the roster.
       */
      members: number;
    }
  | {
      type: "retired";
      /**
       * Vault.
       */
      vault: Hex32;
    }
  | {
      type: "devicePaired";
      /**
       * The device's identity.
       */
      identity: Hex32;
      /**
       * Its name.
       */
      name: string;
    }
  | {
      type: "deviceVote";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * The device.
       */
      identity: Hex32;
      /**
       * Approval or rejection.
       */
      approved: boolean;
      /**
       * Why the vote was refused.
       */
      refused: string | null;
    }
  | {
      type: "deviceUnpaired";
      /**
       * The device's identity.
       */
      identity: Hex32;
    }
  | {
      type: "pendingExpired";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Reshare session.
       */
      session: Hex32;
    };

/**
 * What `daemon.info` reports.
 */
export type DaemonInfo = {
  /**
   * The API version this daemon serves, [`crate::API_VERSION`].
   */
  apiVersion: number;
  /**
   * Daemon release.
   */
  version: string;
  /**
   * Process id.
   */
  pid: number;
  /**
   * Profile name.
   */
  profile: string;
  /**
   * Profile directory.
   */
  profileDir: string;
  /**
   * Unix seconds the daemon started.
   */
  startedAt: number;
  /**
   * Aztec node URL.
   */
  node: string;
};

/**
 * Daemon settings.
 */
export type Settings = {
  /**
   * Aztec node JSON-RPC URL.
   */
  nodeUrl: string;
  /**
   * Relay multiaddrs, each ending in `/p2p/<peer id>`.
   */
  relays: Array<string>;
  /**
   * Prove transactions on this device (off for a local network, which accepts fake proofs).
   */
  proverEnabled: boolean;
  /**
   * Longest signer unlock window.
   */
  unlockTimeoutSecs: number;
  /**
   * Salt of the `SponsoredFPC` instance that pays this member's transactions: zero for the canonical one (local
   * network, Testnet), another for a member's own deployment where none is sponsored (Alpha).
   */
  feePayerSalt: Hex32;
  /**
   * The keeper on another machine this daemon uses instead of running its own.
   */
  remoteKeeper: RemoteKeeper | null;
  /**
   * `aztec-wallet`'s data directory, when not `~/.aztec/wallet`.
   */
  aztecWalletDir: string | null;
  /**
   * Also write vault aliases into `aztec-wallet`'s store (`accounts:<alias>`), so it can pay a vault by name.
   */
  aztecWalletSync: boolean;
};

/**
 * Params of `config.set`: the settings to change.
 */
export type SettingsPatch = {
  /**
   * Aztec node URL.
   */
  nodeUrl?: string;
  /**
   * Prove on this device.
   */
  proverEnabled?: boolean;
  /**
   * Longest unlock window, 60 to 86,400 seconds.
   */
  unlockTimeoutSecs?: number;
  /**
   * Salt of the fee payer's `SponsoredFPC` instance.
   */
  feePayerSalt?: Hex32;
  /**
   * `aztec-wallet`'s data directory; empty for the default.
   */
  aztecWalletDir?: string;
  /**
   * Write vault aliases into `aztec-wallet`'s store.
   */
  aztecWalletSync?: boolean;
};

/**
 * Params of `relays.set`.
 */
export type RelaysParams = {
  /**
   * At most 8 relay multiaddrs.
   */
  relays: Array<string>;
};

/**
 * The member this profile is.
 */
export type Profile = {
  /**
   * Display name.
   */
  name: string;
  /**
   * Public half of the approval signing key; the secret stays sealed in the profile.
   */
  signerKey: Point;
  /**
   * The member's own Aztec account, which pays for deployments.
   */
  account: Hex32;
  /**
   * Unix seconds.
   */
  createdAt: number;
};

/**
 * Params of `profile.create`.
 */
export type ProfileCreateParams = {
  /**
   * Display name, 1 to 48 characters.
   */
  name: string;
  /**
   * Aztec node URL.
   */
  nodeUrl: string;
  /**
   * Passphrase that seals the signer, at least 8 characters.
   */
  passphrase: string;
};

/**
 * A supervised service's state.
 */
export type ServiceStatus = "stopped" | "starting" | "running" | "restarting" | "failed";

/**
 * The member's keeper, as the daemon supervises it.
 */
export type KeeperState = {
  /**
   * Service state.
   */
  status: ServiceStatus;
  /**
   * Keeper identity, once running.
   */
  identity: Hex32 | null;
  /**
   * libp2p peer id, once running.
   */
  peerId: string | null;
  /**
   * Advertised addresses.
   */
  addresses: Array<string>;
  /**
   * Keeper release.
   */
  version: string | null;
  /**
   * Last failure.
   */
  error: string | null;
  /**
   * The remote keeper's `host:port`, when the keeper runs on another machine; `None` for the daemon's own.
   */
  remote: string | null;
};

/**
 * The Aztec node the daemon talks to.
 */
export type NodeStatus = {
  /**
   * Node URL.
   */
  url: string;
  /**
   * Whether the last check reached it.
   */
  reachable: boolean;
  /**
   * Node version.
   */
  nodeVersion: string | null;
  /**
   * L1 chain id.
   */
  l1ChainId: number | null;
  /**
   * Rollup version.
   */
  rollupVersion: number | null;
  /**
   * Latest block.
   */
  blockNumber: number | null;
  /**
   * PXE's synced block.
   */
  syncedBlock: number | null;
  /**
   * Unix seconds of the last check.
   */
  checkedAt: number;
  /**
   * Why it is unreachable.
   */
  error: string | null;
};

/**
 * Whether the signer can sign without a secret.
 */
export type SignerStatus = {
  /**
   * Locked.
   */
  locked: boolean;
  /**
   * Unix seconds an unlock window ends.
   */
  unlockedUntil: number | null;
};

/**
 * How a request unseals the signer: the passphrase, or the sealing key derived from it (`signer.sealingKey`), which
 * lets a client cache the key in a keychain instead of the passphrase.
 */
export type Secret =
  | {
      /**
       * Passphrase.
       */
      passphrase: string;
    }
  | {
      /**
       * 32-byte sealing key.
       */
      key: Hex32;
    };

/**
 * Params of `signer.unlock`.
 */
export type UnlockParams = {
  /**
   * How to unseal.
   */
  secret: Secret;
  /**
   * Window, at most the configured timeout.
   */
  ttlSecs: number;
};

/**
 * Params of `signer.sealingKey`.
 */
export type PassphraseParams = {
  /**
   * Passphrase.
   */
  passphrase: string;
};

/**
 * The result of `signer.sealingKey`.
 */
export type SealingKey = {
  /**
   * Key that unseals the signer in place of the passphrase.
   */
  key: Hex32;
};

/**
 * Everything `ome status` shows.
 */
export type Status = {
  /**
   * The profile, once created.
   */
  profile: Profile | null;
  /**
   * Settings.
   */
  settings: Settings;
  /**
   * The keeper.
   */
  keeper: KeeperState;
  /**
   * The node.
   */
  node: NodeStatus;
  /**
   * The signer.
   */
  signer: SignerStatus;
  /**
   * The member's vaults.
   */
  vaults: Array<VaultSummary>;
  /**
   * The vault commands use when none is named.
   */
  defaultVault: Hex32 | null;
};

/**
 * One of the member's vaults.
 */
export type VaultSummary = {
  /**
   * Vault address.
   */
  address: Hex32;
  /**
   * Name.
   */
  name: string;
  /**
   * Local alias, if set.
   */
  alias: string | null;
  /**
   * The vault commands use when none is named.
   */
  isDefault: boolean;
  /**
   * Spending threshold.
   */
  threshold: number;
  /**
   * Viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Members.
   */
  members: number;
  /**
   * This member's index.
   */
  myIndex: number;
  /**
   * Unix seconds it was founded.
   */
  foundedAt: number;
};

/**
 * The vault contract's deployment.
 */
export type ContractInfo = {
  /**
   * Address.
   */
  address: Hex32;
  /**
   * Preaddress.
   */
  preaddress: Hex32;
  /**
   * Partial address.
   */
  partialAddress: Hex32;
  /**
   * Contract class id.
   */
  classId: Hex32;
  /**
   * Deployment salt.
   */
  salt: Hex32;
  /**
   * Deployer.
   */
  deployer: Hex32;
  /**
   * Initialization hash.
   */
  initializationHash: Hex32;
  /**
   * Public keys by name (`ivpk`, `ovpk`, ...), as hex of their point encoding.
   */
  publicKeys: Array<NamedKey>;
};

/**
 * A named public key.
 */
export type NamedKey = {
  /**
   * Name.
   */
  name: string;
  /**
   * Point.
   */
  key: Point;
};

/**
 * The vault's config note, read from the chain.
 */
export type ConfigView = {
  /**
   * Config version.
   */
  version: number;
  /**
   * Spending threshold.
   */
  threshold: number;
  /**
   * Viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Registered signers.
   */
  signerCount: number;
  /**
   * Signer-set commitment.
   */
  signerSetCommitment: Hex32;
  /**
   * Committee commitment.
   */
  committeeCommitment: Hex32;
  /**
   * Whether it matches the committee the keeper serves.
   */
  matchesCommittee: boolean;
  /**
   * Unix seconds it was read.
   */
  readAt: number;
};

/**
 * A committee member, with the name the member gave themselves.
 */
export type CommitteeMember = {
  /**
   * 1-based index.
   */
  index: number;
  /**
   * Display name.
   */
  name: string;
  /**
   * Approval signing key.
   */
  signerKey: Point;
  /**
   * Verification share.
   */
  verificationShare: Point;
  /**
   * Keeper identity.
   */
  identity: Hex32;
  /**
   * Keeper transport key.
   */
  transport: Hex32;
  /**
   * libp2p peer id.
   */
  peerId: string;
  /**
   * Keeper addresses.
   */
  addresses: Array<string>;
  /**
   * Whether it is this member.
   */
  you: boolean;
};

/**
 * The committee the keeper serves.
 */
export type CommitteeView = {
  /**
   * Epoch.
   */
  epoch: number;
  /**
   * Viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Group key.
   */
  groupKey: Point;
  /**
   * Members.
   */
  members: Array<CommitteeMember>;
  /**
   * Whether the keeper holds its share.
   */
  serving: boolean;
  /**
   * `active`, `retired` or `removed`.
   */
  keeperStatus: string;
};

/**
 * How the vault was founded.
 */
export type FoundingInfo = {
  /**
   * DKG session.
   */
  session: Hex32;
  /**
   * DKG transcript hash.
   */
  transcript: Hex64 | null;
  /**
   * Deployment transaction.
   */
  deployTx: Hex32 | null;
};

/**
 * Everything about one vault.
 */
export type VaultDetails = {
  /**
   * Summary.
   */
  summary: VaultSummary;
  /**
   * Deployment.
   */
  contract: ContractInfo;
  /**
   * On-chain config, once read.
   */
  config: ConfigView | null;
  /**
   * Committee.
   */
  committee: CommitteeView;
  /**
   * Founding.
   */
  founding: FoundingInfo;
};

/**
 * A member's reachability, with their name.
 */
export type MemberReachability = {
  /**
   * Member index.
   */
  index: number;
  /**
   * Display name.
   */
  name: string;
  /**
   * Keeper identity.
   */
  identity: Hex32;
  /**
   * libp2p peer id.
   */
  peerId: string;
  /**
   * Whether it is this member.
   */
  you: boolean;
  /**
   * Connected.
   */
  connected: boolean;
  /**
   * Only through relay circuits.
   */
  relayed: boolean;
  /**
   * Unix seconds of the last contact.
   */
  lastSeen: number | null;
  /**
   * Round trip in milliseconds.
   */
  rttMs: number | null;
};

/**
 * One ECDH audit entry, with the requester's name.
 */
export type AuditView = {
  /**
   * Unix seconds.
   */
  at: number;
  /**
   * `local`, or the requesting keeper's identity.
   */
  requester: string;
  /**
   * The requesting member's name, if known.
   */
  requesterName: string | null;
  /**
   * What the points were for.
   */
  purpose: string;
  /**
   * Number of points.
   */
  points: number;
  /**
   * `served`, or why it was refused.
   */
  outcome: string;
};

/**
 * A token the member knows.
 */
export type TokenInfo = {
  /**
   * Token contract.
   */
  address: Hex32;
  /**
   * Name.
   */
  name: string;
  /**
   * Symbol.
   */
  symbol: string;
  /**
   * Decimals.
   */
  decimals: number;
};

/**
 * The vault's private balance of a token.
 */
export type TokenBalance = {
  /**
   * Token.
   */
  token: TokenInfo;
  /**
   * Balance, if it could be read.
   */
  balance: Amount | null;
  /**
   * Why it could not.
   */
  error: string | null;
};

/**
 * Something that happened to a vault.
 */
export type ActivityEntry =
  | {
      kind: "proposal";
      /**
       * Unix seconds.
       */
      at: number;
      /**
       * The proposal.
       */
      proposal: ProposalView;
    }
  | {
      kind: "received";
      /**
       * Unix seconds.
       */
      at: number;
      /**
       * Token.
       */
      token: TokenInfo;
      /**
       * Sender, as the note reveals it.
       */
      from: Hex32;
      /**
       * Amount.
       */
      amount: Amount;
      /**
       * Transaction.
       */
      txHash: Hex32;
      /**
       * Block.
       */
      block: number;
    };

/**
 * A lobby's host, as an invite names it.
 */
export type InviteHost = {
  /**
   * Keeper identity.
   */
  identity: Hex32;
  /**
   * libp2p peer id.
   */
  peerId: string;
  /**
   * Keeper addresses.
   */
  addresses: Array<string>;
};

/**
 * What an invite says, checked against this daemon's network.
 */
export type InvitePreview = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * Vault name.
   */
  name: string;
  /**
   * Host's name.
   */
  hostName: string;
  /**
   * Proposed spending threshold.
   */
  threshold: number;
  /**
   * Proposed viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Network.
   */
  network: string;
  /**
   * Whether it is this daemon's network.
   */
  sameNetwork: boolean;
  /**
   * The host.
   */
  host: InviteHost;
};

/**
 * A member on a lobby's roster.
 */
export type LobbyViewMember = {
  /**
   * Future member index.
   */
  index: number;
  /**
   * Name.
   */
  name: string;
  /**
   * Approval signing key.
   */
  signerKey: Point;
  /**
   * Keeper identity.
   */
  identity: Hex32;
  /**
   * libp2p peer id.
   */
  peerId: string;
  /**
   * Keeper addresses.
   */
  addresses: Array<string>;
  /**
   * Whether it is this member.
   */
  you: boolean;
};

/**
 * A lobby.
 */
export type LobbyView = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * This member's part.
   */
  role: LobbyRole;
  /**
   * Where it is.
   */
  status: LobbyStatus;
  /**
   * Vault name.
   */
  name: string;
  /**
   * Spending threshold.
   */
  threshold: number;
  /**
   * Viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Network.
   */
  network: string;
  /**
   * Roster.
   */
  members: Array<LobbyViewMember>;
  /**
   * The invite, on the host.
   */
  invite: string | null;
  /**
   * The DKG session, once started.
   */
  session: Hex32 | null;
};

/**
 * Params of `lobby.create`.
 */
export type LobbyCreateParams = {
  /**
   * Vault name, 1 to 48 characters.
   */
  name: string;
  /**
   * Proposed spending threshold, 1 to 8.
   */
  threshold: number;
  /**
   * Proposed viewing threshold, 2 to 8.
   */
  viewingThreshold: number;
};

/**
 * Params of `lobby.start`.
 */
export type StartLobbyParams = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * Final spending threshold.
   */
  threshold: number;
  /**
   * Final viewing threshold.
   */
  viewingThreshold: number;
};

/**
 * A founding step.
 */
export type FoundingStepId = "agree" | "dkg" | "derive" | "register" | "deploy" | "verify";

/**
 * Where a step is.
 */
export type StepStatus = "pending" | "active" | "done" | "failed";

/**
 * One founding step.
 */
export type FoundingStep = {
  /**
   * Which.
   */
  id: FoundingStepId;
  /**
   * Where it is.
   */
  status: StepStatus;
  /**
   * Unix seconds it started.
   */
  startedAt: number | null;
  /**
   * Unix seconds it finished.
   */
  finishedAt: number | null;
  /**
   * Short detail, e.g. "round 2 of 3" or the deployment transaction.
   */
  detail: string | null;
};

/**
 * Why a founding failed.
 */
export type FoundingError = {
  /**
   * Failed step.
   */
  step: FoundingStepId;
  /**
   * Message.
   */
  message: string;
  /**
   * Identities held responsible.
   */
  blamed: Array<string>;
};

/**
 * A founding in progress or finished.
 */
export type FoundingState = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * Steps in order.
   */
  steps: Array<FoundingStep>;
  /**
   * The vault, once derived.
   */
  vault: Hex32 | null;
  /**
   * Failure.
   */
  error: FoundingError | null;
  /**
   * Finished, successfully or not.
   */
  done: boolean;
};

/**
 * One call of a `calls` intent.
 */
export type IntentCall = {
  /**
   * Contract.
   */
  to: Hex32;
  /**
   * Function name.
   */
  fn: string;
  /**
   * Arguments as the CLI parses them.
   */
  args: Array<string>;
};

/**
 * What a proposal is for. Carried as JSON in the proposal's summary (at most 4 KiB).
 */
export type Intent =
  | {
      kind: "transfer";
      /**
       * Token.
       */
      token: Hex32;
      /**
       * Recipient.
       */
      to: Hex32;
      /**
       * Amount.
       */
      amount: Amount;
      /**
       * Note for the members, at most 280 characters.
       */
      memo: string | null;
    }
  | {
      kind: "calls";
      /**
       * Calls.
       */
      calls: Array<IntentCall>;
      /**
       * Note.
       */
      memo: string | null;
    }
  | {
      kind: "cancel";
      /**
       * The proposal cancelled.
       */
      cancels: Hex32;
      /**
       * Note.
       */
      memo: string | null;
    };

/**
 * Where a proposal is.
 */
export type ProposalStatus =
  | "collecting"
  | "ready"
  | "blocked"
  | "executed"
  | "cancelled"
  | "expired"
  | "stale";

/**
 * A member's approval or rejection.
 */
export type Vote = {
  /**
   * Signer slot, 0-based.
   */
  slot: number;
  /**
   * Member index.
   */
  index: number;
  /**
   * Name.
   */
  name: string;
  /**
   * Whether the signature verifies.
   */
  valid: boolean;
  /**
   * Reason, for rejections.
   */
  reason: string | null;
};

/**
 * A call as the payload encodes it.
 */
export type DecodedCall = {
  /**
   * Contract.
   */
  to: Hex32;
  /**
   * Its label, from the address book, tokens or members.
   */
  toLabel: string | null;
  /**
   * Function.
   */
  fn: string;
  /**
   * Arguments.
   */
  args: Array<string>;
  /**
   * Static call.
   */
  isStatic: boolean;
};

/**
 * The proposer.
 */
export type Proposer = {
  /**
   * Member index.
   */
  index: number;
  /**
   * Name.
   */
  name: string;
};

/**
 * An execution.
 */
export type Execution = {
  /**
   * Transaction.
   */
  txHash: Hex32;
  /**
   * Unix seconds.
   */
  at: number;
};

/**
 * A proposal as this member sees it.
 */
export type ProposalView = {
  /**
   * Proposal id.
   */
  id: Hex32;
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * What it is for.
   */
  intent: Intent;
  /**
   * Who proposed it.
   */
  proposer: Proposer;
  /**
   * Where it is.
   */
  status: ProposalStatus;
  /**
   * Spending threshold.
   */
  threshold: number;
  /**
   * Members.
   */
  members: number;
  /**
   * Approvals.
   */
  approvals: Array<Vote>;
  /**
   * Valid rejections from slots that have not also approved.
   */
  rejections: Array<Vote>;
  /**
   * This member's slot.
   */
  mySlot: number;
  /**
   * Whether this member approved.
   */
  approvedByMe: boolean;
  /**
   * Whether this member rejected.
   */
  rejectedByMe: boolean;
  /**
   * Config version it was made under.
   */
  configVersion: number;
  /**
   * The vault's current config version.
   */
  currentConfigVersion: number;
  /**
   * Unix seconds it expires.
   */
  expiresAt: number;
  /**
   * Transaction nonce.
   */
  txNonce: Hex32;
  /**
   * Outer hash the approvals sign over.
   */
  outerHash: Hex32;
  /**
   * The message an approval signs.
   */
  approvalMessage: Hex32;
  /**
   * How fees are paid.
   */
  feeMethod: string;
  /**
   * Decoded calls.
   */
  calls: Array<DecodedCall>;
  /**
   * For transfers, the token as this member knows it.
   */
  token: TokenInfo | null;
  /**
   * The execution, once executed.
   */
  execution: Execution | null;
  /**
   * The cancel proposal targeting it, or the proposal a cancel targets.
   */
  cancelledBy: Hex32 | null;
};

/**
 * Gas a simulation used.
 */
export type Gas = {
  /**
   * DA gas.
   */
  da: number;
  /**
   * L2 gas.
   */
  l2: number;
};

/**
 * Checks a review ran on this device.
 */
export type ReviewChecks = {
  /**
   * The payload was rebuilt from the intent and matches.
   */
  rebuilt: boolean;
  /**
   * The recomputed outer hash.
   */
  outerHash: Hex32;
  /**
   * Made under the current config.
   */
  configCurrent: boolean;
  /**
   * Not expired.
   */
  notExpired: boolean;
  /**
   * What failed, if anything.
   */
  failure: string | null;
};

/**
 * Where a recipient's label came from.
 */
export type LabelSource = "aztecWallet" | "member";

/**
 * A transfer's recipient.
 */
export type Recipient = {
  /**
   * Address.
   */
  address: Hex32;
  /**
   * Label, if known.
   */
  label: string | null;
  /**
   * Where the label came from.
   */
  source: LabelSource | null;
};

/**
 * A simulation.
 */
export type Simulation = {
  /**
   * Succeeded.
   */
  ok: boolean;
  /**
   * Gas used.
   */
  gas: Gas | null;
  /**
   * Why it failed.
   */
  error: string | null;
};

/**
 * What a signature covers.
 */
export type Signs = {
  /**
   * The message.
   */
  message: Hex32;
  /**
   * How it is computed.
   */
  formula: string;
};

/**
 * What `ome approve` shows before the member signs, all rebuilt and checked on this device.
 */
export type Review = {
  /**
   * The proposal.
   */
  proposal: ProposalView;
  /**
   * Vault name.
   */
  vaultName: string;
  /**
   * This member's name.
   */
  myName: string;
  /**
   * Checks.
   */
  checks: ReviewChecks;
  /**
   * One line, e.g. "transfer 25 JADE (25000000000000000000 base units)".
   */
  action: string;
  /**
   * For transfers.
   */
  recipient: Recipient | null;
  /**
   * Simulation.
   */
  simulation: Simulation;
  /**
   * What an approval signs.
   */
  signs: Signs;
  /**
   * What a rejection signs.
   */
  rejects: Signs;
};

/**
 * A dry run of a proposal before it is made.
 */
export type ProposalPreview = {
  /**
   * Succeeded.
   */
  ok: boolean;
  /**
   * Calls it would make.
   */
  calls: Array<DecodedCall>;
  /**
   * Gas used.
   */
  gas: Gas | null;
  /**
   * Why it failed.
   */
  error: string | null;
};

/**
 * Params naming a proposal.
 */
export type ProposalRef = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Proposal id.
   */
  id: Hex32;
};

/**
 * Params of `proposals.preview` and `proposals.create`.
 */
export type ProposeParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * What to do.
   */
  intent: Intent;
  /**
   * Validity, 1 to 720 hours.
   */
  expiresInHours: number;
};

/**
 * Params of `proposals.approve`.
 */
export type ApproveParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Proposal id.
   */
  id: Hex32;
  /**
   * Unseals the signer for this call when it is locked.
   */
  secret?: Secret;
};

/**
 * Params of `proposals.reject`.
 */
export type RejectParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Proposal id.
   */
  id: Hex32;
  /**
   * Reason shown to the members, at most 280 characters.
   */
  reason?: string;
  /**
   * Unseals the signer for this call when it is locked.
   */
  secret?: Secret;
};

/**
 * Where an execution on this device is.
 */
export type ExecutionPhase = "rebuilding" | "proving" | "sending" | "mined" | "failed";

/**
 * Progress of an execution on this device.
 */
export type ExecutionProgress = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Proposal.
   */
  proposal: Hex32;
  /**
   * Phase.
   */
  phase: ExecutionPhase;
  /**
   * Why it failed.
   */
  error: string | null;
  /**
   * Transaction, once sent.
   */
  txHash: Hex32 | null;
};

/**
 * A function of a contract, for building `calls` intents.
 */
export type ContractFunction = {
  /**
   * Name.
   */
  name: string;
  /**
   * `private`, `public` or `utility`.
   */
  kind: FunctionKind;
  /**
   * Static.
   */
  isStatic: boolean;
  /**
   * Parameters.
   */
  params: Array<FunctionParam>;
};

/**
 * Where a function runs.
 */
export type FunctionKind = "private" | "public" | "utility";

/**
 * A function parameter.
 */
export type FunctionParam = {
  /**
   * Name.
   */
  name: string;
  /**
   * ABI type, as text.
   */
  type: string;
};

/**
 * A member to add, as a verified join request names them.
 */
export type Joiner = {
  /**
   * Name.
   */
  name: string;
  /**
   * Approval signing key.
   */
  signerKey: Point;
  /**
   * Their keeper.
   */
  keeper: KeeperContact;
};

/**
 * Params of `membership.plan`.
 */
export type MembershipParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Member indices to remove.
   */
  remove: Array<number>;
  /**
   * Join requests (`membership.request` on the newcomers' daemons) of the members to add.
   */
  add: Array<string>;
  /**
   * New spending threshold.
   */
  threshold: number;
  /**
   * New viewing threshold.
   */
  viewingThreshold: number;
};

/**
 * A committee's shape before or after a rotation.
 */
export type CommitteeShape = {
  /**
   * Spending threshold.
   */
  threshold: number;
  /**
   * Viewing threshold.
   */
  viewingThreshold: number;
  /**
   * Member names in index order.
   */
  members: Array<string>;
};

/**
 * A planned rotation, checked on this device.
 */
export type MembershipPlan = {
  /**
   * Rotation id.
   */
  id: Hex32;
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Now.
   */
  current: CommitteeShape;
  /**
   * After.
   */
  next: CommitteeShape;
  /**
   * Members that deal sub-shares: members who stay.
   */
  dealers: Array<number>;
  /**
   * The members joining, from their verified join requests.
   */
  joiners: Array<Joiner>;
  /**
   * Whether it removes at most `t_v - 1` members.
   */
  withinRemovalLimit: boolean;
  /**
   * Why it cannot run, if it cannot.
   */
  failure: string | null;
  /**
   * The plan to hand every member who stays and every joiner (`ome-rotation-v0:…`); each passes it to
   * `membership.begin`. It carries the reshare's parameters and the vault's deployment, no secrets.
   */
  share: string;
};

/**
 * Params naming a rotation.
 */
export type RotationRef = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Rotation id.
   */
  id: Hex32;
};

/**
 * Params of `membership.begin`.
 */
export type RotationPlanText = {
  /**
   * A plan from `membership.plan`, on this or another member's daemon.
   */
  plan: string;
};

/**
 * A rotation step.
 */
export type RotationStepId = "reshare" | "propose" | "approve" | "execute" | "activate";

/**
 * One rotation step.
 */
export type RotationStep = {
  /**
   * Which.
   */
  id: RotationStepId;
  /**
   * Where it is.
   */
  status: StepStatus;
  /**
   * Short detail.
   */
  detail: string | null;
};

/**
 * A rotation in progress or finished.
 */
export type RotationState = {
  /**
   * The plan.
   */
  plan: MembershipPlan;
  /**
   * Steps in order.
   */
  steps: Array<RotationStep>;
  /**
   * The reshare session, once started.
   */
  session: Hex32 | null;
  /**
   * The `rotate_membership` proposal, once made.
   */
  proposal: Hex32 | null;
  /**
   * Failure.
   */
  error: string | null;
  /**
   * Finished, successfully or not.
   */
  done: boolean;
};

/**
 * Something a client should redraw.
 */
export type DaemonEvent =
  | {
      type: "keeper";
      /**
       * New state.
       */
      keeper: KeeperState;
    }
  | {
      type: "keeperEvent";
      /**
       * The keeper's event.
       */
      event: KeeperEvent;
    }
  | {
      type: "signer";
      /**
       * New status.
       */
      signer: SignerStatus;
    }
  | {
      type: "node";
      /**
       * New status.
       */
      node: NodeStatus;
    }
  | {
      type: "lobby";
      /**
       * The lobby.
       */
      lobby: LobbyView;
    }
  | {
      type: "founding";
      /**
       * Its state.
       */
      founding: FoundingState;
    }
  | {
      type: "vaultActivity";
      /**
       * Vault.
       */
      vault: Hex32;
    }
  | {
      type: "execution";
      /**
       * Progress.
       */
      execution: ExecutionProgress;
    }
  | {
      type: "rotation";
      /**
       * Its state.
       */
      rotation: RotationState;
    }
  | {
      type: "handshakeAdvised";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Exposed counterparties.
       */
      counterparties: number;
    };

/**
 * Params of `vault.audit`.
 */
export type VaultAuditParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Most recent entries, 1 to 500.
   */
  limit: number;
};

/**
 * Params of `vault.alias`.
 */
export type AliasParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Lowercase letters, digits, `-` and `_`, at most 32.
   */
  alias: string;
};

/**
 * Params of `tokens.add`.
 */
export type TokenParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Token contract.
   */
  token: Hex32;
  /**
   * How the token was deployed, for a token whose instance was never published: the daemon derives the instance
   * from it and refuses one that derives another address.
   */
  deployment?: TokenDeployment;
};

/**
 * Params of `contracts.functions`.
 */
export type ContractParams = {
  /**
   * Contract.
   */
  contract: Hex32;
};

/**
 * A device paired with this member's keeper.
 */
export type DeviceView = {
  /**
   * The device's identity.
   */
  identity: Hex32;
  /**
   * Its name.
   */
  name: string;
  /**
   * The key it signs approvals with.
   */
  signerKey: Point;
  /**
   * When it paired (unix seconds).
   */
  pairedAt: number;
  /**
   * Its libp2p peer id, for a private relay's allow list.
   */
  peerId: string;
  /**
   * Vaults where its key holds this member's slot: proposals there are sent to it for review.
   */
  vaults: Array<Hex32>;
};

/**
 * Params of `devices.enroll`: move this member's signing key in `vault` to a paired device.
 */
export type EnrollParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * The paired device's identity.
   */
  identity: Hex32;
};

/**
 * Params of `membership.swapSigner`: replace one slot's signing key, keeping the committee and thresholds.
 */
export type SwapSignerParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Signer slot (member index - 1).
   */
  slot: number;
  /**
   * The new key, from the member's new device.
   */
  signerKey: Point;
};

/**
 * A standard token's deployment: the `Token` constructor's arguments, its salt and its deployer.
 */
export type TokenDeployment = {
  /**
   * Deployment salt.
   */
  salt: Hex32;
  /**
   * Deployer address (zero for a universal deployment).
   */
  deployer: Hex32;
  /**
   * The constructor's `admin`.
   */
  admin: Hex32;
  /**
   * Name.
   */
  name: string;
  /**
   * Symbol.
   */
  symbol: string;
  /**
   * Decimals.
   */
  decimals: number;
};

/**
 * Someone who paid the vault, from the transfers it received.
 */
export type Counterparty = {
  /**
   * Sender.
   */
  address: Hex32;
  /**
   * Its name in `aztec-wallet`'s aliases, if any.
   */
  label: string | null;
  /**
   * Transfers received from it.
   */
  received: number;
  /**
   * First transfer (unix seconds).
   */
  firstAt: number;
  /**
   * Latest transfer (unix seconds).
   */
  lastAt: number;
  /**
   * It paid the vault before a member was removed: that member can still tell when it pays again, until it
   * handshakes with the vault anew.
   */
  exposed: boolean;
};

/**
 * A keeper on another machine (`ome-keeper run --rpc-listen`), reached over TLS.
 */
export type RemoteKeeper = {
  /**
   * `host:port`.
   */
  address: string;
  /**
   * SHA-256 fingerprint of its certificate (hex), as the keeper prints it.
   */
  fingerprint: string;
};

/**
 * A remote keeper with its RPC token (`<data>/rpc.token` on the keeper's machine).
 */
export type RemoteKeeperParams = {
  /**
   * `host:port`.
   */
  address: string;
  /**
   * SHA-256 fingerprint of its certificate (hex).
   */
  fingerprint: string;
  /**
   * The keeper's RPC token; the daemon keeps it sealed.
   */
  token: string;
};

/**
 * Params of `keeper.connect`: a remote keeper, or `null` for the daemon's own.
 */
export type KeeperConnectParams = {
  /**
   * The keeper to use from now on.
   */
  remote: RemoteKeeperParams | null;
};

/**
 * An alias in `aztec-wallet`'s store, such as `accounts:alice` or `contracts:usdc`.
 */
export type AztecAlias = {
  /**
   * `<type>:<name>`.
   */
  key: string;
  /**
   * What it stands for: an address, or an artifact path.
   */
  value: string;
};

/**
 * Params of `tokens.remove`.
 */
export type TokenRefParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Token contract.
   */
  token: Hex32;
};

/** Every keeper method: what it takes and returns. */
export interface KeeperMethods {
  "session.auth": { params: AuthParams; result: null };
  "events.subscribe": { params: Empty; result: null };
  "keeper.info": { params: Empty; result: KeeperInfo };
  "keeper.joinRequest": { params: LobbyMemberParams; result: JoinRequestText };
  "vault.list": { params: Empty; result: Array<Vault> };
  "vault.status": { params: VaultParams; result: Vault };
  "vault.register": { params: RegisterParams; result: Vault };
  "vault.observeConfig": { params: ObserveConfigParams; result: Observed };
  "vault.retire": { params: RetireParams; result: Vault };
  "vault.updateSigners": { params: UpdateSignersParams; result: Vault };
  "vault.sync": { params: VaultParams; result: null };
  "vault.observeTx": { params: TxParams; result: TxObserved };
  "peers.status": { params: VaultParams; result: Array<Reachability> };
  "mailbox.poll": { params: Empty; result: MailboxPolled };
  "ecdh.batch": { params: EcdhParams; result: EcdhResult };
  "ceremony.dkg": { params: DkgParams; result: null };
  "ceremony.reshare": { params: ReshareParams; result: null };
  "ceremony.status": { params: SessionParams; result: Ceremony };
  "proposals.put": { params: BodyParams; result: ProposalStored };
  "proposals.list": { params: VaultParams; result: Array<HexBytes> };
  "proposals.get": { params: IdParams; result: HexBytes | null };
  "payloads.put": { params: BodyParams; result: PayloadStored };
  "payloads.get": { params: PayloadParams; result: HexBytes | null };
  "approvals.put": { params: BodyParams; result: null };
  "approvals.list": { params: ProposalParams; result: Array<HexBytes> };
  "rejections.put": { params: BodyParams; result: null };
  "rejections.list": { params: ProposalParams; result: Array<HexBytes> };
  "receipts.put": { params: ReceiptParams; result: null };
  "receipts.list": { params: VaultParams; result: Array<Receipt> };
  "audit.list": { params: AuditParams; result: Array<AuditEntry> };
  "lobby.open": { params: LobbyOpenParams; result: LobbyOpened };
  "lobby.join": { params: LobbyJoinParams; result: LobbyJoined };
  "lobby.inspect": { params: InviteParams; result: Invite };
  "lobby.status": { params: LobbyParams; result: Lobby };
  "lobby.list": { params: Empty; result: Array<Lobby> };
  "lobby.start": { params: LobbyStartParams; result: Lobby };
  "lobby.close": { params: LobbyParams; result: Lobby };
  "lobby.leave": { params: LobbyParams; result: Lobby };
  "devices.offer": { params: DeviceOfferParams; result: DeviceOffer };
  "devices.list": { params: Empty; result: Array<Device> };
  "devices.revoke": { params: DeviceParams; result: null };
  "devices.review": { params: DeviceReviewParams; result: DeviceReviewSent };
}

/** Every daemon method: what it takes and returns. */
export interface DaemonMethods {
  "session.auth": { params: AuthParams; result: null };
  "events.subscribe": { params: Empty; result: null };
  "daemon.info": { params: Empty; result: DaemonInfo };
  "daemon.stop": { params: Empty; result: null };
  status: { params: Empty; result: Status };
  "profile.create": { params: ProfileCreateParams; result: Status };
  "config.get": { params: Empty; result: Settings };
  "config.set": { params: SettingsPatch; result: Settings };
  "relays.set": { params: RelaysParams; result: Settings };
  "keeper.connect": { params: KeeperConnectParams; result: KeeperState };
  "signer.status": { params: Empty; result: SignerStatus };
  "signer.unlock": { params: UnlockParams; result: SignerStatus };
  "signer.lock": { params: Empty; result: SignerStatus };
  "signer.sealingKey": { params: PassphraseParams; result: SealingKey };
  "node.status": { params: Empty; result: NodeStatus };
  "lobby.inspect": { params: InviteParams; result: InvitePreview };
  "lobby.create": { params: LobbyCreateParams; result: LobbyView };
  "lobby.join": { params: InviteParams; result: LobbyView };
  "lobby.get": { params: LobbyParams; result: LobbyView };
  "lobby.list": { params: Empty; result: Array<LobbyView> };
  "lobby.start": { params: StartLobbyParams; result: LobbyView };
  "lobby.leave": { params: LobbyParams; result: null };
  "founding.begin": { params: LobbyParams; result: FoundingState };
  "founding.get": { params: LobbyParams; result: FoundingState | null };
  "vault.list": { params: Empty; result: Array<VaultSummary> };
  "vault.details": { params: VaultParams; result: VaultDetails };
  "vault.peers": { params: VaultParams; result: Array<MemberReachability> };
  "vault.audit": { params: VaultAuditParams; result: Array<AuditView> };
  "vault.activity": { params: VaultParams; result: Array<ActivityEntry> };
  "vault.counterparties": { params: VaultParams; result: Array<Counterparty> };
  "vault.alias": { params: AliasParams; result: Array<VaultSummary> };
  "vault.retire": { params: VaultParams; result: Array<VaultSummary> };
  "vault.use": { params: VaultParams; result: Array<VaultSummary> };
  "tokens.list": { params: VaultParams; result: Array<TokenBalance> };
  "tokens.add": { params: TokenParams; result: TokenInfo };
  "tokens.remove": { params: TokenRefParams; result: Array<TokenInfo> };
  "tokens.deployTest": { params: VaultParams; result: TokenInfo };
  "proposals.list": { params: VaultParams; result: Array<ProposalView> };
  "proposals.get": { params: ProposalRef; result: ProposalView };
  "proposals.preview": { params: ProposeParams; result: ProposalPreview };
  "proposals.create": { params: ProposeParams; result: ProposalView };
  "proposals.review": { params: ProposalRef; result: Review };
  "proposals.approve": { params: ApproveParams; result: ProposalView };
  "proposals.reject": { params: RejectParams; result: ProposalView };
  "proposals.execute": { params: ProposalRef; result: ProposalView };
  "proposals.cancel": { params: ProposalRef; result: ProposalView };
  "contracts.functions": { params: ContractParams; result: Array<ContractFunction> };
  "aztec.aliases": { params: Empty; result: Array<AztecAlias> };
  "aztec.alias": { params: AztecAlias; result: null };
  "membership.request": { params: Empty; result: JoinRequestText };
  "membership.plan": { params: MembershipParams; result: MembershipPlan };
  "membership.begin": { params: RotationPlanText; result: RotationState };
  "membership.get": { params: RotationRef; result: RotationState | null };
  "membership.list": { params: VaultParams; result: Array<RotationState> };
  "membership.swapSigner": { params: SwapSignerParams; result: ProposalView };
  "devices.pair": { params: Empty; result: DeviceOffer };
  "devices.list": { params: Empty; result: Array<DeviceView> };
  "devices.revoke": { params: DeviceParams; result: null };
  "devices.enroll": { params: EnrollParams; result: ProposalView };
  "devices.forward": { params: ProposalRef; result: DeviceReviewSent };
}

/** JSON-RPC error code of each kind. */
export const ERROR_CODES = {
  parse: -32700,
  "invalid-request": -32600,
  "method-not-found": -32601,
  invalid: -32602,
  unauthorized: -32001,
  "not-found": -32014,
  "not-ready": -32010,
  locked: -32011,
  auth: -32012,
  alarm: -32013,
  failed: -32000,
} as const satisfies Record<ErrorKind, number>;

/** API version both services report. */
export const API_VERSION = 0;
