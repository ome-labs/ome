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
  | "insufficient-funds"
  | "expired"
  | "stale"
  | "conflict"
  | "full"
  | "unreadable"
  | "unknown-contract"
  | "network"
  | "failed";

/**
 * What a `not-ready` call waits for.
 */
export type NotReadyReason =
  | "starting"
  | "profile"
  | "signer"
  | "node"
  | "keeper"
  | "sync"
  | "vault"
  | "payload"
  | "reshare"
  | "members";

/**
 * What a transaction is short of.
 */
export type Funds =
  | {
      type: "token";
      /**
       * The token.
       */
      token: TokenInfo;
    }
  | {
      type: "feeJuice";
      /**
       * The fee payer.
       */
      payer: Hex32;
    };

/**
 * The details of an error, one variant per kind that has them.
 */
export type ErrorDetails =
  | {
      kind: "not-ready";
      /**
       * What isn't ready.
       */
      reason: NotReadyReason;
    }
  | {
      kind: "insufficient-funds";
      /**
       * What is short.
       */
      funds: Funds;
      /**
       * What the transaction needs, in base units.
       */
      needed: Amount;
      /**
       * What is held, in base units.
       */
      held: Amount;
    }
  | {
      kind: "expired";
      /**
       * Unix seconds.
       */
      at: number;
    }
  | {
      kind: "stale";
      /**
       * Config version in the proposal.
       */
      made: number;
      /**
       * The vault's current config version.
       */
      current: number;
    }
  | {
      kind: "conflict";
      /**
       * The member.
       */
      member: MemberRef;
      /**
       * How far they are.
       */
      phase: ExecutionPhase;
      /**
       * Unix seconds they started.
       */
      since: number;
    }
  | {
      kind: "full";
      /**
       * Members it takes.
       */
      members: number;
    }
  | {
      kind: "unreadable";
      /**
       * Keepers that answered with valid partials.
       */
      answered: number;
      /**
       * The vault's viewing threshold.
       */
      needed: number;
    }
  | {
      kind: "unknown-contract";
      /**
       * Its address.
       */
      address: Hex32;
      /**
       * Its class, when the node knows the instance.
       */
      classId?: Hex32;
    }
  | {
      kind: "network";
      /**
       * Where it belongs, e.g. `aztec:11155111:679261766`.
       */
      expected: string;
      /**
       * This profile's network.
       */
      actual: string;
    };

/**
 * The `data` member of an error response.
 */
export type ErrorData = {
  /**
   * What went wrong.
   */
  kind: ErrorKind;
  /**
   * The numbers needed to fix it, for kinds that have them. Details a client doesn't understand read as absent.
   */
  details?: ErrorDetails;
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
 * Params of `receipts.put`: this keeper's member executed `proposal`, and the keeper signs the receipt.
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
  /**
   * Block that included it.
   */
  block: number;
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
  /**
   * Block that included it.
   */
  block: number;
  /**
   * The executing member's index in the current committee; `None` when its keeper is no longer a member.
   */
  executor: number | null;
};

/**
 * A stored approval or rejection, with when this keeper first stored it.
 */
export type StoredVote = {
  /**
   * Canonical encoding.
   */
  body: HexBytes;
  /**
   * Unix seconds this keeper stored it.
   */
  at: number;
};

/**
 * Params of `claims.put`: this keeper's member is executing `proposal`.
 */
export type ClaimParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Proposal being executed.
   */
  proposal: Hex32;
  /**
   * How far it is; `mined` or `failed` ends the claim.
   */
  phase: ExecutionPhase;
  /**
   * Unix seconds the execution started.
   */
  startedAt: number;
};

/**
 * A member executing a proposal, as a claim this keeper holds.
 */
export type Claim = {
  /**
   * Proposal being executed.
   */
  proposal: Hex32;
  /**
   * The executing member's index.
   */
  member: number;
  /**
   * How far it is.
   */
  phase: ExecutionPhase;
  /**
   * Unix seconds the execution started.
   */
  startedAt: number;
  /**
   * Unix seconds of its last update; a claim lapses `CLAIM_TTL_SECS` after it.
   */
  updatedAt: number;
  /**
   * Whether it is this keeper's member.
   */
  mine: boolean;
};

/**
 * What a rename names.
 */
export type NameSubject = "vault" | "member";

/**
 * Params of `names.put`: this keeper's member renames the vault, or themselves.
 */
export type NamePutParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * What it names.
   */
  subject: NameSubject;
  /**
   * The new name, 1 to 64 bytes.
   */
  name: string;
};

/**
 * The current name of a subject.
 */
export type Name = {
  /**
   * What it names.
   */
  subject: NameSubject;
  /**
   * The name.
   */
  name: string;
  /**
   * When it was set (the renaming keeper's unix seconds).
   */
  at: number;
  /**
   * The renaming member's keeper identity (for a member's name, the member).
   */
  by: Hex32;
  /**
   * The renaming member's index in the current committee, when still a member.
   */
  member: number | null;
};

/**
 * Params of `lobby.remove`: the host drops a joiner before starting.
 */
export type LobbyRemoveParams = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * The joiner's keeper identity.
   */
  identity: Hex32;
};

/**
 * Params of `lobby.update`: the host changes the plan before starting.
 */
export type LobbyUpdateParams = {
  /**
   * Lobby id.
   */
  lobby: Hex16;
  /**
   * How many members the vault will have.
   */
  members?: number;
  /**
   * Spending threshold.
   */
  threshold?: number;
  /**
   * Viewing threshold.
   */
  viewingThreshold?: number;
};

/**
 * What changed in a lobby.
 */
export type LobbyChangeKind =
  | "opened"
  | "joined"
  | "left"
  | "removed"
  | "updated"
  | "started"
  | "closed";

/**
 * A change to a lobby, and the member it was about.
 */
export type LobbyChange = {
  /**
   * What changed.
   */
  kind: LobbyChangeKind;
  /**
   * The member who joined, left or was removed (keeper identity).
   */
  member: Hex32 | null;
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
   * The member's own Aztec account, shown to the other members.
   */
  account: Hex32;
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
   * How many members the vault will have, host included (2 to 8).
   */
  members: number;
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
   * How many members the vault will have, host included.
   */
  members: number;
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
   * The member's own Aztec account.
   */
  account: Hex32;
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
   * How many members the vault will have, host included.
   */
  expected: number;
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
   * This keeper's member was removed by the host (the lobby is then `closed` for it).
   */
  removed: boolean;
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
 * Params of `artifacts.put`: one chunk of an artifact this member's daemon holds.
 */
export type ArtifactPutParams = {
  /**
   * The contract class.
   */
  classId: Hex32;
  /**
   * Chunk index, from 0.
   */
  index: number;
  /**
   * Chunks in the artifact.
   */
  count: number;
  /**
   * The chunk: a slice of the gzip-compressed artifact JSON.
   */
  bytes: HexBytes;
};

/**
 * Params of `artifacts.get`.
 */
export type ArtifactChunkParams = {
  /**
   * The contract class.
   */
  classId: Hex32;
  /**
   * Chunk index, from 0.
   */
  index: number;
};

/**
 * One stored chunk.
 */
export type ArtifactChunk = {
  /**
   * Chunks in the artifact.
   */
  count: number;
  /**
   * The chunk.
   */
  bytes: HexBytes;
};

/**
 * Params of `artifacts.fetch` and `artifacts.status`.
 */
export type ArtifactParams = {
  /**
   * The vault whose members are asked for it.
   */
  vault: Hex32;
  /**
   * The contract class.
   */
  classId: Hex32;
};

/**
 * How much of an artifact this keeper holds.
 */
export type ArtifactStatus = {
  /**
   * Every chunk is here.
   */
  complete: boolean;
  /**
   * Chunks held.
   */
  have: number;
  /**
   * Chunks in the artifact, once known.
   */
  count: number | null;
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
      type: "artifact";
      /**
       * The vault whose members were asked.
       */
      vault: Hex32;
      /**
       * The contract class.
       */
      classId: Hex32;
      /**
       * Every chunk arrived and matched the manifest.
       */
      complete: boolean;
      /**
       * Why it stopped, when not complete.
       */
      error: string | null;
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
      /**
       * The executing member's index, when its keeper is a member.
       */
      executor: number | null;
    }
  | {
      type: "name";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * The current name.
       */
      name: Name;
    }
  | {
      type: "claim";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * The executing member's index.
       */
      member: number;
      /**
       * How far it is; `mined` or `failed` ends the claim.
       */
      phase: ExecutionPhase;
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
      /**
       * What changed, when it was one member or step.
       */
      change: LobbyChange | null;
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
  /**
   * What the daemon accepts.
   */
  limits: Limits;
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
  /**
   * Answer app connections (`apps.*`); off by default.
   */
  appsEnabled: boolean;
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
  /**
   * Answer app connections.
   */
  appsEnabled?: boolean;
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
  /**
   * The fee payer and its Fee Juice, once the daemon has a wallet.
   */
  fees: FeesStatus | null;
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
  /**
   * Its open proposals at a glance; refreshed in the background.
   */
  counts: ProposalCounts;
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
   * The member's own Aztec account, when this member's daemon learned it (lobbies and join requests carry it).
   */
  account: Hex32 | null;
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
   * The member's own Aztec account, when known.
   */
  account: Hex32 | null;
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
 * A member of a vault, by slot and name.
 */
export type MemberRef = {
  /**
   * Member index, from 1.
   */
  index: number;
  /**
   * Name.
   */
  name: string;
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
  /**
   * Aztec's standard `Token` contract; `false` for a token with its own code.
   */
  standard: boolean;
};

/**
 * The vault's balance of a token, private (its notes) and public (the token's storage).
 */
export type TokenBalance = {
  /**
   * Token.
   */
  token: TokenInfo;
  /**
   * Private balance: needs the keepers to read the vault's notes.
   */
  private: BalanceState;
  /**
   * Public balance: needs no viewing key, so it shows even when the keepers can't answer.
   */
  public: BalanceState;
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
   * How many members the vault will have, host included.
   */
  members: number;
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
   * The member's own Aztec account.
   */
  account: Hex32;
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
   * How many members the vault will have, host included; the host starts once that many have joined.
   */
  expected: number;
  /**
   * Network.
   */
  network: string;
  /**
   * Roster.
   */
  members: Array<LobbyViewMember>;
  /**
   * The host removed this member (the lobby is then `closed` for them).
   */
  removed: boolean;
  /**
   * The invite, on the host; a new one after a removal.
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
  /**
   * How many members the vault will have, host included, 2 to 8.
   */
  members: number;
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
   * Arguments, one per parameter, each checked against its type (`contracts.functions`).
   */
  args: Array<AbiValue>;
};

/**
 * A private authwit a `calls` intent grants: `caller`, a contract the transaction reaches (an AMM the calls invoke,
 * directly or through a router), may make `call` on the vault's behalf once, inside the transaction. The
 * transaction must use it: a proposal whose simulation never asks the vault about it is refused.
 */
export type IntentAuthwit = {
  /**
   * The contract allowed to make the call.
   */
  caller: Hex32;
  /**
   * The private call it may make; its contract consumes the authwit.
   */
  call: IntentCall;
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
       * Private authwits granted with the calls, at most `daemon.info.limits.maxAuthwits`.
       */
      authwits?: Array<IntentAuthwit>;
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
    }
  | {
      kind: "setThreshold";
      /**
       * The new threshold, from 1 to the member count.
       */
      threshold: number;
      /**
       * Note.
       */
      memo: string | null;
    }
  | {
      kind: "invalidatePending";
      /**
       * Note.
       */
      memo: string | null;
    }
  | {
      kind: "app";
      /**
       * The app's origin.
       */
      origin: string;
      /**
       * The app's name, as the member allowed it.
       */
      name: string | null;
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
   * An approval, or a rejection.
   */
  approve: boolean;
  /**
   * Whether the signature verifies.
   */
  valid: boolean;
  /**
   * Reason, for rejections.
   */
  reason: string | null;
  /**
   * Unix seconds this member's keeper first stored it.
   */
  at: number;
};

/**
 * A call as the payload encodes it, decoded with the contract's artifact when this member has it.
 */
export type DecodedCall = {
  /**
   * Contract.
   */
  to: Hex32;
  /**
   * Its label: a token, the vault, or an aztec-wallet alias.
   */
  toLabel: string | null;
  /**
   * Function.
   */
  fn: string;
  /**
   * Selector, `0x` and 8 hex characters.
   */
  selector: string;
  /**
   * Where it runs.
   */
  kind: FunctionKind;
  /**
   * Arguments, named and typed when `decoded`.
   */
  args: Array<DecodedArg>;
  /**
   * Static call.
   */
  isStatic: boolean;
  /**
   * Whether the arguments were decoded with the contract's artifact; until it arrives they are raw fields.
   */
  decoded: boolean;
};

/**
 * A private authwit a proposal grants, decoded from its payload.
 */
export type DecodedAuthwit = {
  /**
   * The contract allowed to make the call.
   */
  caller: Hex32;
  /**
   * Its label, as for a call's target.
   */
  callerLabel: string | null;
  /**
   * The call it may make.
   */
  call: DecodedCall;
  /**
   * The authwit's message hash, which the vault approves.
   */
  messageHash: Hex32;
  /**
   * The message an approval signs for it.
   */
  approvalMessage: Hex32;
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
  /**
   * The member who executed it, from the receipt their keeper signed; `None` when they are no longer a member.
   */
  executor: MemberRef | null;
  /**
   * Block that included it.
   */
  block: number | null;
};

/**
 * A member executing a proposal right now.
 */
export type ExecutionView = {
  /**
   * Who.
   */
  member: MemberRef;
  /**
   * How far they are.
   */
  phase: ExecutionPhase;
  /**
   * Unix seconds they started.
   */
  since: number;
  /**
   * Whether it is this member.
   */
  mine: boolean;
};

/**
 * Params of `proposals.execute`.
 */
export type ExecuteParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Proposal id.
   */
  id: Hex32;
  /**
   * Execute even while another member is: both prove, and the proposal's nonce lets one land.
   */
  force?: boolean;
};

/**
 * The fee payer and its Fee Juice.
 */
export type FeesStatus = {
  /**
   * The fee payer contract.
   */
  payer: Hex32;
  /**
   * Whose it is.
   */
  kind: FeePayerKind;
  /**
   * Its Fee Juice, in base units; `None` when the node can't be read.
   */
  feeJuice: Amount | null;
  /**
   * It holds Fee Juice to pay with.
   */
  healthy: boolean;
};

/**
 * Whose fee payer it is.
 */
export type FeePayerKind = "sponsored" | "own";

/**
 * What a transaction is expected to cost, from its simulation and the node's current fees.
 */
export type FeeEstimate = {
  /**
   * Gas the simulation used.
   */
  gas: Gas;
  /**
   * Its cost at the node's current minimum fees, in Fee Juice base units.
   */
  fee: Amount;
  /**
   * The most it may cost: Aztec's wallet pads the fee per gas by half against fee changes before inclusion.
   */
  maxFee: Amount;
  /**
   * Who pays.
   */
  payer: Hex32;
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
   * Approvals, then valid rejections from slots that have not also approved, each with its time.
   */
  votes: Array<Vote>;
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
   * Unix seconds the proposer made it (the clock `expires_at` uses).
   */
  createdAt: number;
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
   * The private authwits it grants, each signed with the transaction.
   */
  authwits: Array<DecodedAuthwit>;
  /**
   * For transfers, the token as this member knows it.
   */
  token: TokenInfo | null;
  /**
   * The execution, once executed.
   */
  execution: Execution | null;
  /**
   * A member executing it right now.
   */
  executing: ExecutionView | null;
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
  failure: ReviewError | null;
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
  error: SimulationError | null;
  /**
   * What it does to the vault, when it succeeded.
   */
  effects: Effects | null;
  /**
   * What it is expected to cost, when it succeeded.
   */
  fee: FeeEstimate | null;
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
   * For governance intents: the vault's policy before and after.
   */
  change: GovernanceChange | null;
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
   * Authwits it would grant.
   */
  authwits: Array<DecodedAuthwit>;
  /**
   * Gas used.
   */
  gas: Gas | null;
  /**
   * Why it failed.
   */
  error: SimulationError | null;
  /**
   * What it would do to the vault, when it succeeded.
   */
  effects: Effects | null;
  /**
   * What it is expected to cost, when it succeeded.
   */
  fee: FeeEstimate | null;
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
 * A Noir ABI type, complete enough to draw a form for any parameter and to check a value against it.
 */
export type AbiType =
  | { kind: "field" }
  | { kind: "boolean" }
  | {
      kind: "integer";
      /**
       * Signed.
       */
      signed: boolean;
      /**
       * Bits.
       */
      width: number;
    }
  | {
      kind: "array";
      /**
       * Elements.
       */
      length: number;
      /**
       * Element type.
       */
      item: AbiType;
    }
  | {
      kind: "string";
      /**
       * Most characters.
       */
      length: number;
    }
  | {
      kind: "struct";
      /**
       * Its Noir path, e.g. `aztec::protocol_types::address::aztec_address::AztecAddress`.
       */
      path: string;
      /**
       * Its fields, in order.
       */
      fields: Array<AbiField>;
      /**
       * Set for structs that are one value to a person (an address, a selector), so forms and reviews show them as
       * one value.
       */
      known?: KnownType;
    }
  | {
      kind: "tuple";
      /**
       * Its items, in order.
       */
      items: Array<AbiType>;
    };

/**
 * A struct that is one value to a person.
 */
export type KnownType = "aztecAddress" | "ethAddress" | "functionSelector";

/**
 * A struct field.
 */
export type AbiField = {
  /**
   * Name.
   */
  name: string;
  /**
   * Type.
   */
  type: AbiType;
};

/**
 * An argument value, checked against its [`AbiType`]: text for fields, integers (decimal, or `0x` hex), addresses and
 * strings; a boolean; a list for arrays and tuples; an object for structs. A known struct takes text.
 */
export type AbiValue = boolean | string | Array<AbiValue> | { [key in string]: AbiValue };

/**
 * One argument of a decoded call.
 */
export type DecodedArg = {
  /**
   * Parameter name; `arg0`, `arg1`… when the call couldn't be decoded.
   */
  name: string;
  /**
   * Parameter type; a field when the call couldn't be decoded.
   */
  type: AbiType;
  /**
   * The value.
   */
  value: AbiValue;
  /**
   * How a person reads it, when the daemon knows better than the raw value: `bob (alias)`, `75.5 OMET`.
   */
  display: string | null;
};

/**
 * Params of `contracts.functions`.
 */
export type ContractFunctionsParams = {
  /**
   * Contract.
   */
  contract: Hex32;
  /**
   * Include utility (read-only) functions, which a proposal can't call. Off when absent.
   */
  includeUtility?: boolean;
};

/**
 * Where this member's knowledge of a contract comes from.
 */
export type ContractSource = "registered" | "token" | "vault";

/**
 * A contract this member can call.
 */
export type ContractView = {
  /**
   * Address.
   */
  address: Hex32;
  /**
   * The artifact's name.
   */
  name: string;
  /**
   * Contract class.
   */
  classId: Hex32;
  /**
   * Where it comes from.
   */
  source: ContractSource;
  /**
   * Functions a proposal can call.
   */
  functions: number;
};

/**
 * What an unpublished contract instance derives from, to register it.
 */
export type ContractInstanceInput = {
  /**
   * Deployment salt.
   */
  salt: Hex32;
  /**
   * Deployer, zero for a universal deployment.
   */
  deployer: Hex32;
  /**
   * Initialization hash.
   */
  initializationHash: Hex32;
  /**
   * Hash of the immutable values the contract was deployed with.
   */
  immutablesHash: Hex32;
  /**
   * Public keys, serialized as Aztec does.
   */
  publicKeys: HexBytes;
};

/**
 * Params of `contracts.register`.
 */
export type ContractRegisterParams = {
  /**
   * The artifact, as JSON text.
   */
  artifact: string;
  /**
   * The contract's address.
   */
  address: Hex32;
  /**
   * Needed only when the node has no published instance at `address`.
   */
  instance?: ContractInstanceInput;
};

/**
 * What the daemon accepts, so clients don't hard-code it.
 */
export type Limits = {
  /**
   * Calls per proposal besides the fee payer's.
   */
  maxCalls: number;
  /**
   * Private authwits a proposal grants.
   */
  maxAuthwits: number;
  /**
   * Bytes of a proposal's summary, which carries its intent.
   */
  maxSummaryBytes: number;
  /**
   * Bytes of a contract artifact, uncompressed.
   */
  maxArtifactBytes: number;
  /**
   * Characters of a memo or rejection reason.
   */
  memoChars: number;
  /**
   * Fewest hours a proposal can last.
   */
  minExpiryHours: number;
  /**
   * Most hours a proposal can last.
   */
  maxExpiryHours: number;
};

/**
 * An app the member allowed to use a vault.
 */
export type AppView = {
  /**
   * The app's web origin, `https://app.example` (scheme, host and port).
   */
  origin: string;
  /**
   * The vault it sees as its only account.
   */
  vault: Hex32;
  /**
   * A name the member gave it.
   */
  name: string | null;
  /**
   * Unix seconds it was allowed.
   */
  allowedAt: number;
};

/**
 * Params of `apps.allow`.
 */
export type AppAllowParams = {
  /**
   * The app's origin.
   */
  origin: string;
  /**
   * The vault it may use.
   */
  vault: Hex32;
  /**
   * A name for it, at most 64 bytes.
   */
  name?: string;
};

/**
 * Params naming an app.
 */
export type AppParams = {
  /**
   * The app's origin.
   */
  origin: string;
};

/**
 * Params of `apps.call`: one wallet call from an allowed app.
 */
export type AppCallParams = {
  /**
   * The app's origin.
   */
  origin: string;
  /**
   * The `Wallet` method, e.g. `simulateTx`.
   */
  method: string;
  /**
   * Its arguments as aztec.js encodes them (`jsonStringify` of the argument list).
   */
  args: string;
};

/**
 * The result of `apps.call`, as aztec.js encodes it (`jsonStringify`).
 */
export type AppCallResult = {
  /**
   * The method's result.
   */
  result: string;
};

/**
 * Whether a balance is private (notes) or public (storage).
 */
export type Visibility = "private" | "public";

/**
 * A tracked token's balance in the vault, before and after the transaction.
 */
export type BalanceChange = {
  /**
   * The token.
   */
  token: TokenInfo;
  /**
   * Before, in base units.
   */
  before: Amount;
  /**
   * After, in base units.
   */
  after: Amount;
  /**
   * Which balance.
   */
  visibility: Visibility;
};

/**
 * What one call returns.
 */
export type CallResult = {
  /**
   * Index into the transaction's calls.
   */
  call: number;
  /**
   * Its return values, decoded when the member has the contract's artifact.
   */
  returns: Array<DecodedArg>;
};

/**
 * Notes the transaction creates and spends, against Aztec's per-transaction limits.
 */
export type NoteCounts = {
  /**
   * Note hashes created.
   */
  created: number;
  /**
   * Nullifiers emitted.
   */
  nullified: number;
};

/**
 * What a simulation shows the transaction does to the vault.
 */
export type Effects = {
  /**
   * Tracked tokens whose vault balance changes.
   */
  balances: Array<BalanceChange>;
  /**
   * Calls that return something.
   */
  returns: Array<CallResult>;
  /**
   * Notes created and spent.
   */
  notes: NoteCounts;
  /**
   * False when some change to the vault couldn't be attributed (a note of an untracked contract, or of a shape the
   * daemon doesn't read): the transaction may change other balances.
   */
  complete: boolean;
};

/**
 * Why a review's checks failed.
 */
export type ReviewFailure =
  | { kind: "payloadMismatch" }
  | {
      kind: "configChanged";
      /**
       * Config version in the proposal.
       */
      made: number;
      /**
       * The vault's current config version.
       */
      current: number;
    }
  | {
      kind: "expired";
      /**
       * Unix seconds.
       */
      at: number;
    }
  | {
      kind: "artifactMissing";
      /**
       * The contract class.
       */
      classId: Hex32 | null;
    }
  | { kind: "notMember" }
  | { kind: "feeOnVault" }
  | { kind: "invalid" };

/**
 * A review failure with its message.
 */
export type ReviewError = {
  /**
   * What failed.
   */
  failure: ReviewFailure;
  /**
   * In words, for people.
   */
  message: string;
};

/**
 * Why a simulation failed.
 */
export type SimulationFailure =
  | {
      kind: "insufficientBalance";
      /**
       * The token, when tracked.
       */
      token: TokenInfo | null;
      /**
       * The token contract.
       */
      contract: Hex32;
    }
  | {
      kind: "authwitRequired";
      /**
       * The contract that asked.
       */
      contract: Hex32 | null;
    }
  | {
      kind: "unusedAuthwit";
      /**
       * Its position in the proposal's authwits.
       */
      index: number;
      /**
       * The contract that would have consumed it.
       */
      contract: Hex32;
    }
  | { kind: "nonceUsed" }
  | {
      kind: "reverted";
      /**
       * The contract, when known.
       */
      contract: Hex32 | null;
      /**
       * The function, when known.
       */
      function: string | null;
    }
  | { kind: "unavailable" };

/**
 * A simulation failure with its message.
 */
export type SimulationError = {
  /**
   * What failed.
   */
  failure: SimulationFailure;
  /**
   * In words, for people; for `reverted`, the contract's own message.
   */
  message: string;
};

/**
 * A threshold choice members should make knowingly (`ome-protocol`'s policy warnings).
 */
export type PolicyWarning =
  | "single-signer"
  | "no-spare-signer"
  | "two-can-read"
  | "no-spare-viewer";

/**
 * A change to the vault's own policy, as a review shows it.
 */
export type GovernanceChange =
  | {
      kind: "threshold";
      /**
       * Now.
       */
      before: number;
      /**
       * After.
       */
      after: number;
      /**
       * Members.
       */
      members: number;
      /**
       * Risks of the new shape.
       */
      warnings: Array<PolicyWarning>;
    }
  | {
      kind: "invalidatePending";
      /**
       * Config version now.
       */
      version: number;
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
   * Selector, `0x` and 8 hex characters.
   */
  selector: string;
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
  /**
   * Return types.
   */
  returns: Array<AbiType>;
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
   * Type.
   */
  type: AbiType;
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
   * The newcomer's own Aztec account.
   */
  account: Hex32;
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
      /**
       * What changed, when it was one member or step.
       */
      change: LobbyChange | null;
    }
  | {
      type: "vaultRenamed";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * The new name.
       */
      name: string;
      /**
       * Who renamed it.
       */
      by: MemberRef;
    }
  | {
      type: "memberRenamed";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * The member, under the new name.
       */
      member: MemberRef;
    }
  | { type: "sealingKeyChanged" }
  | {
      type: "founding";
      /**
       * Its state.
       */
      founding: FoundingState;
    }
  | {
      type: "appConnect";
      /**
       * The app's origin.
       */
      origin: string;
    }
  | {
      type: "appRequest";
      /**
       * The app's origin.
       */
      origin: string;
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * The proposal.
       */
      proposal: Hex32;
    }
  | {
      type: "proposalCreated";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * One line: "Transfer 40 OMET to 0xf941…12b3".
       */
      summary: string;
      /**
       * Who proposed it.
       */
      by: MemberRef;
    }
  | {
      type: "proposalVoted";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * One line.
       */
      summary: string;
      /**
       * Who voted.
       */
      by: MemberRef;
      /**
       * Approved, or rejected.
       */
      approve: boolean;
      /**
       * The rejection's reason.
       */
      reason: string | null;
    }
  | {
      type: "proposalReady";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * One line.
       */
      summary: string;
    }
  | {
      type: "proposalBlocked";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * One line.
       */
      summary: string;
    }
  | {
      type: "proposalExecuting";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * One line.
       */
      summary: string;
      /**
       * Who.
       */
      by: MemberRef;
      /**
       * How far they are.
       */
      phase: ExecutionPhase;
    }
  | {
      type: "proposalExecuted";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * One line.
       */
      summary: string;
      /**
       * Transaction.
       */
      txHash: Hex32;
      /**
       * Who executed it, when their keeper is a member.
       */
      by: MemberRef | null;
    }
  | {
      type: "proposalCancelled";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * The cancelled proposal.
       */
      proposal: Hex32;
      /**
       * One line.
       */
      summary: string;
      /**
       * The cancellation that ran.
       */
      byProposal: Hex32;
    }
  | {
      type: "proposalExpiring";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * One line.
       */
      summary: string;
      /**
       * Unix seconds it expires.
       */
      at: number;
    }
  | {
      type: "proposalExpired";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Proposal id.
       */
      proposal: Hex32;
      /**
       * One line.
       */
      summary: string;
    }
  | {
      type: "execution";
      /**
       * Progress.
       */
      execution: ExecutionProgress;
    }
  | {
      type: "received";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Token.
       */
      token: TokenInfo;
      /**
       * Amount, in base units.
       */
      amount: Amount;
      /**
       * Sender.
       */
      from: Hex32;
      /**
       * Transaction.
       */
      txHash: Hex32;
    }
  | {
      type: "balanceChanged";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Token contract.
       */
      token: Hex32;
    }
  | {
      type: "contractAvailable";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * The contract class.
       */
      classId: Hex32;
    }
  | {
      type: "tokenAdded";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Token.
       */
      token: TokenInfo;
    }
  | {
      type: "tokenRemoved";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * Token contract.
       */
      token: Hex32;
    }
  | {
      type: "vaultUpdated";
      /**
       * Vault.
       */
      vault: Hex32;
    }
  | {
      type: "configChanged";
      /**
       * Vault.
       */
      vault: Hex32;
      /**
       * The config version now.
       */
      version: number;
    }
  | {
      type: "memberChanged";
      /**
       * Vault.
       */
      vault: Hex32;
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
 * Where an event sits in the daemon's stream: `epoch` changes each time the daemon starts, `n` counts up from 1.
 */
export type EventSeq = {
  /**
   * The daemon start this stream belongs to.
   */
  epoch: number;
  /**
   * The event's number in that stream.
   */
  n: number;
};

/**
 * An event as it is sent: numbered and timed.
 */
export type EventEnvelope = {
  /**
   * Its place in the stream.
   */
  seq: EventSeq;
  /**
   * Unix seconds it happened.
   */
  at: number;
  /**
   * What happened.
   */
  event: DaemonEvent;
};

/**
 * Params of `events.subscribe`.
 */
export type SubscribeParams = {
  /**
   * The last event the client saw, to receive the ones after it first.
   */
  since?: EventSeq;
};

/**
 * What `events.subscribe` answers before the stream starts.
 */
export type Subscription = {
  /**
   * The current stream.
   */
  epoch: number;
  /**
   * The last event sent so far in it (0 before the first).
   */
  latest: number;
  /**
   * The events after `since` can't be replayed (another epoch, or too old): refetch everything once.
   */
  reset: boolean;
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
 * A balance as this member could read it.
 */
export type BalanceState =
  | {
      state: "known";
      /**
       * In base units.
       */
      amount: Amount;
    }
  | {
      state: "unreadable";
      /**
       * Keepers that answered with valid partials.
       */
      answered: number;
      /**
       * The vault's viewing threshold.
       */
      needed: number;
    }
  | {
      state: "failed";
      /**
       * Why.
       */
      message: string;
    };

/**
 * What the Receive page can promise.
 */
export type ReceivingStatus = {
  /**
   * Payments arriving with a non-interactive handshake (any Aztec wallet's default) are found.
   */
  handshakes: boolean;
  /**
   * Senders registered from `aztec-wallet`'s accounts, found even without a handshake.
   */
  senders: number;
  /**
   * The latest transfer received in a tracked token (unix seconds).
   */
  lastReceivedAt: number | null;
};

/**
 * Params of `vault.request`.
 */
export type PaymentRequestParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * The token asked for.
   */
  token?: Hex32;
  /**
   * The amount asked for, in base units.
   */
  amount?: Amount;
  /**
   * What it is for.
   */
  memo?: string;
};

/**
 * A payment request to hand a payer: `ome-pay:` link (`architecture/wire.md`).
 */
export type PaymentRequest = {
  /**
   * The `ome-pay:` link.
   */
  uri: string;
  /**
   * The vault.
   */
  vault: Hex32;
  /**
   * The network: `aztec:<L1 chain id>:<rollup version>`.
   */
  network: string;
  /**
   * The token asked for, when the vault tracks it.
   */
  token: TokenInfo | null;
  /**
   * The amount, in base units.
   */
  amount: Amount | null;
  /**
   * What it is for.
   */
  memo: string | null;
};

/**
 * Which proposals `proposals.list` returns.
 */
export type ProposalFilter = "all" | "open" | "ready" | "done" | "needsMe";

/**
 * Params of `proposals.list`.
 */
export type ProposalListParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Which proposals; all when absent.
   */
  filter?: ProposalFilter;
  /**
   * At most this many, 1 to 500 (50 when absent).
   */
  limit?: number;
  /**
   * The previous page's `next`: items older than it.
   */
  before?: string;
};

/**
 * What `vault.activity` lists.
 */
export type ActivityKind = "proposal" | "received";

/**
 * Params of `vault.activity`.
 */
export type ActivityParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * Which kinds; every kind when absent.
   */
  kinds?: Array<ActivityKind>;
  /**
   * Only entries about this token.
   */
  token?: Hex32;
  /**
   * At most this many, 1 to 500 (50 when absent).
   */
  limit?: number;
  /**
   * The previous page's `next`.
   */
  before?: string;
};

/**
 * Params of `vault.audit` and `vault.counterparties`.
 */
export type PageParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * At most this many, 1 to 500 (50 when absent).
   */
  limit?: number;
  /**
   * The previous page's `next`.
   */
  before?: string;
};

/**
 * A page of proposals, newest first. `next` is the cursor for older ones, `None` on the last page.
 */
export type ProposalPage = {
  /**
   * Proposals.
   */
  items: Array<ProposalView>;
  /**
   * Cursor for the next page.
   */
  next: string | null;
};

/**
 * A page of activity, newest first.
 */
export type ActivityPage = {
  /**
   * Entries.
   */
  items: Array<ActivityEntry>;
  /**
   * Cursor for the next page.
   */
  next: string | null;
};

/**
 * A page of the ECDH audit log, newest first.
 */
export type AuditPage = {
  /**
   * Entries.
   */
  items: Array<AuditView>;
  /**
   * Cursor for the next page.
   */
  next: string | null;
};

/**
 * A page of counterparties, most recent payer first.
 */
export type CounterpartyPage = {
  /**
   * Counterparties.
   */
  items: Array<Counterparty>;
  /**
   * Cursor for the next page.
   */
  next: string | null;
};

/**
 * A vault's open proposals at a glance, for a badge.
 */
export type ProposalCounts = {
  /**
   * Collecting, ready or blocked.
   */
  open: number;
  /**
   * Ready to execute.
   */
  ready: number;
  /**
   * Collecting or ready, and this member hasn't voted.
   */
  needsMe: number;
};

/**
 * Params of `vault.rename`.
 */
export type RenameParams = {
  /**
   * Vault.
   */
  vault: Hex32;
  /**
   * The vault's new name, 1 to 48 characters, for every member.
   */
  name: string;
};

/**
 * Params of `signer.changePassphrase`.
 */
export type ChangePassphraseParams = {
  /**
   * The current passphrase, or the sealing key.
   */
  current: Secret;
  /**
   * The new passphrase, at least 8 characters.
   */
  next: string;
};

/**
 * Params of `profile.rename`.
 */
export type ProfileRenameParams = {
  /**
   * The member's display name, 1 to 48 characters; every vault's members see it.
   */
  name: string;
};

/**
 * Params of `profile.delete`.
 */
export type ProfileDeleteParams = {
  /**
   * The profile's directory name, typed again to confirm.
   */
  confirm: string;
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
 * Params naming a contract (`contracts.remove`).
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
  "artifacts.put": { params: ArtifactPutParams; result: ArtifactStatus };
  "artifacts.get": { params: ArtifactChunkParams; result: ArtifactChunk | null };
  "artifacts.status": { params: ArtifactParams; result: ArtifactStatus };
  "artifacts.fetch": { params: ArtifactParams; result: ArtifactStatus };
  "approvals.put": { params: BodyParams; result: null };
  "approvals.list": { params: ProposalParams; result: Array<StoredVote> };
  "rejections.put": { params: BodyParams; result: null };
  "rejections.list": { params: ProposalParams; result: Array<StoredVote> };
  "receipts.put": { params: ReceiptParams; result: null };
  "receipts.list": { params: VaultParams; result: Array<Receipt> };
  "claims.put": { params: ClaimParams; result: null };
  "claims.list": { params: VaultParams; result: Array<Claim> };
  "names.put": { params: NamePutParams; result: Name };
  "names.list": { params: VaultParams; result: Array<Name> };
  "audit.list": { params: AuditParams; result: Array<AuditEntry> };
  "lobby.open": { params: LobbyOpenParams; result: LobbyOpened };
  "lobby.join": { params: LobbyJoinParams; result: LobbyJoined };
  "lobby.inspect": { params: InviteParams; result: Invite };
  "lobby.status": { params: LobbyParams; result: Lobby };
  "lobby.list": { params: Empty; result: Array<Lobby> };
  "lobby.start": { params: LobbyStartParams; result: Lobby };
  "lobby.close": { params: LobbyParams; result: Lobby };
  "lobby.leave": { params: LobbyParams; result: Lobby };
  "lobby.remove": { params: LobbyRemoveParams; result: Lobby };
  "lobby.update": { params: LobbyUpdateParams; result: Lobby };
  "devices.offer": { params: DeviceOfferParams; result: DeviceOffer };
  "devices.list": { params: Empty; result: Array<Device> };
  "devices.revoke": { params: DeviceParams; result: null };
  "devices.review": { params: DeviceReviewParams; result: DeviceReviewSent };
}

/** Every daemon method: what it takes and returns. */
export interface DaemonMethods {
  "session.auth": { params: AuthParams; result: null };
  "events.subscribe": { params: SubscribeParams; result: Subscription };
  "daemon.info": { params: Empty; result: DaemonInfo };
  "daemon.stop": { params: Empty; result: null };
  status: { params: Empty; result: Status };
  "profile.create": { params: ProfileCreateParams; result: Status };
  "profile.rename": { params: ProfileRenameParams; result: Profile };
  "profile.delete": { params: ProfileDeleteParams; result: null };
  "config.get": { params: Empty; result: Settings };
  "config.set": { params: SettingsPatch; result: Settings };
  "relays.set": { params: RelaysParams; result: Settings };
  "keeper.connect": { params: KeeperConnectParams; result: KeeperState };
  "signer.status": { params: Empty; result: SignerStatus };
  "signer.unlock": { params: UnlockParams; result: SignerStatus };
  "signer.lock": { params: Empty; result: SignerStatus };
  "signer.sealingKey": { params: PassphraseParams; result: SealingKey };
  "signer.changePassphrase": { params: ChangePassphraseParams; result: SignerStatus };
  "node.status": { params: Empty; result: NodeStatus };
  "fees.status": { params: Empty; result: FeesStatus };
  "lobby.inspect": { params: InviteParams; result: InvitePreview };
  "lobby.create": { params: LobbyCreateParams; result: LobbyView };
  "lobby.join": { params: InviteParams; result: LobbyView };
  "lobby.get": { params: LobbyParams; result: LobbyView };
  "lobby.list": { params: Empty; result: Array<LobbyView> };
  "lobby.start": { params: StartLobbyParams; result: LobbyView };
  "lobby.leave": { params: LobbyParams; result: null };
  "lobby.remove": { params: LobbyRemoveParams; result: LobbyView };
  "lobby.update": { params: LobbyUpdateParams; result: LobbyView };
  "founding.begin": { params: LobbyParams; result: FoundingState };
  "founding.get": { params: LobbyParams; result: FoundingState | null };
  "vault.list": { params: Empty; result: Array<VaultSummary> };
  "vault.details": { params: VaultParams; result: VaultDetails };
  "vault.peers": { params: VaultParams; result: Array<MemberReachability> };
  "vault.audit": { params: PageParams; result: AuditPage };
  "vault.activity": { params: ActivityParams; result: ActivityPage };
  "vault.counterparties": { params: PageParams; result: CounterpartyPage };
  "vault.receiving": { params: VaultParams; result: ReceivingStatus };
  "vault.request": { params: PaymentRequestParams; result: PaymentRequest };
  "vault.alias": { params: AliasParams; result: Array<VaultSummary> };
  "vault.rename": { params: RenameParams; result: VaultSummary };
  "vault.retire": { params: VaultParams; result: Array<VaultSummary> };
  "vault.use": { params: VaultParams; result: Array<VaultSummary> };
  "tokens.list": { params: VaultParams; result: Array<TokenBalance> };
  "tokens.add": { params: TokenParams; result: TokenInfo };
  "tokens.remove": { params: TokenRefParams; result: Array<TokenInfo> };
  "tokens.deployTest": { params: VaultParams; result: TokenInfo };
  "proposals.list": { params: ProposalListParams; result: ProposalPage };
  "proposals.get": { params: ProposalRef; result: ProposalView };
  "proposals.preview": { params: ProposeParams; result: ProposalPreview };
  "proposals.create": { params: ProposeParams; result: ProposalView };
  "proposals.review": { params: ProposalRef; result: Review };
  "proposals.approve": { params: ApproveParams; result: ProposalView };
  "proposals.reject": { params: RejectParams; result: ProposalView };
  "proposals.execute": { params: ExecuteParams; result: ProposalView };
  "proposals.cancel": { params: ProposalRef; result: ProposalView };
  "contracts.functions": { params: ContractFunctionsParams; result: Array<ContractFunction> };
  "contracts.register": { params: ContractRegisterParams; result: ContractView };
  "contracts.list": { params: Empty; result: Array<ContractView> };
  "contracts.remove": { params: ContractParams; result: null };
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
  "apps.list": { params: Empty; result: Array<AppView> };
  "apps.allow": { params: AppAllowParams; result: AppView };
  "apps.revoke": { params: AppParams; result: null };
  "apps.call": { params: AppCallParams; result: AppCallResult };
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
  "insufficient-funds": -32015,
  expired: -32016,
  stale: -32017,
  conflict: -32018,
  full: -32019,
  unreadable: -32020,
  "unknown-contract": -32021,
  network: -32022,
  failed: -32000,
} as const satisfies Record<ErrorKind, number>;

/** API version both services report. */
export const API_VERSION = 0;
