export type TimelineReaction = {
  emoji: string;
  /** Custom (image) emoji URL from the reaction's NIP-30 `emoji` tag, if any. */
  emojiUrl?: string;
  count: number;
  reactedByCurrentUser?: boolean;
  users: Array<{
    pubkey: string;
    displayName: string;
    avatarUrl: string | null;
  }>;
};

/**
 * Explanation of an agent session transition (see `lib/sessionStatus.ts`).
 * Copy only: built from the bounded wire vocabulary, never raw tag text.
 */
export type SessionStatusExplanation = {
  title: string;
  detail?: string;
  history?: string[];
};

/**
 * Derived, self-reported session status of an agent's own message. Present
 * only when there is something to show; `percent` is absent (never 0) when
 * the agent reported no context occupancy.
 */
export type MessageSessionStatus = {
  /** Only on the earliest loaded message of its transition. */
  explanation?: SessionStatusExplanation;
  percent?: number;
};

export type TimelineMessage = {
  id: string;
  /** Stable local key used to avoid remounting optimistic rows on send ack. */
  renderKey?: string;
  createdAt: number;
  pubkey?: string;
  /**
   * Raw signer pubkey (`event.pubkey`), normalized to lowercase hex.
   * Distinct from `pubkey`, which may be a delegated author on an event signed
   * by the active relay. Use this field for checks that require the process or
   * user that cryptographically signed the event.
   */
  signerPubkey?: string;
  author: string;
  /** True when the displayed author is known to be an agent. */
  isAgent?: boolean;
  /** Verified owner pubkey for an agent author, when available. */
  ownerPubkey?: string | null;
  /** Viewer-relative owner label (for example, "you" or "baxen"). */
  ownerLabel?: string | null;
  avatarUrl?: string | null;
  role?: string;
  /** For bot messages, the display name of the persona this bot was created from. */
  personaDisplayName?: string;
  /** For bot messages, the respond-to mode (who can interact with this bot). */
  respondTo?: "owner-only" | "allowlist" | "anyone";
  time: string;
  body: string;
  parentId?: string | null;
  rootId?: string | null;
  depth: number;
  accent?: boolean;
  pending?: boolean;
  edited?: boolean;
  highlighted?: boolean;
  kind?: number;
  tags?: string[][];
  reactions?: TimelineReaction[];
  /** Derived from the original signed tags; see `lib/sessionStatus.ts`. */
  sessionStatus?: MessageSessionStatus;
};
