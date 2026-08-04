/**
 * NIP-74 event builders (TypeScript) — mirrors core/src/lib.rs builders.
 * Returns UNSIGNED event JSON; signing is the caller's job (NIP-46 remote
 * signer, or the Rust core crate).
 */

export const KIND_SERVICE_BOND = 37_000;
export const KIND_BOND_ACCEPTANCE = 37_001;
export const KIND_PERFORMANCE_ATTESTATION = 37_002;
export const KIND_BOND_CLAIM = 37_003;
export const KIND_ARBITRATION_VERDICT = 37_004;
export const KIND_BOND_CANCELLATION = 37_005;
export const KIND_LABEL = 1985;
export const KIND_HANDLER_ANNOUNCEMENT = 31_990;

export interface UnsignedEvent {
  pubkey: string;
  created_at: number;
  kind: number;
  tags: string[][];
  content: string;
}

export function bondAddress(agentPubkey: string, bondId: string): string {
  return `37000:${agentPubkey}:${bondId}`;
}

export function serviceBond(opts: {
  agentPubkey: string;
  channel: string;
  witnesses: string[];
  taskType: string;
  bondId: string;
  amount: string;
  asset: string;
  deadline: number;
  createdAt: number;
  description: string;
}): UnsignedEvent {
  const tags: string[][] = [
    ["d", opts.bondId],
    ["p", opts.agentPubkey],
    ["h", opts.channel],
    ["t", opts.taskType],
    ["amount", opts.amount],
    ["asset", opts.asset],
    ["chain", "solana"],
    ["deadline", String(opts.deadline)],
    ...opts.witnesses.map((w) => ["witness", w] as string[]),
  ];
  return {
    pubkey: opts.agentPubkey,
    created_at: opts.createdAt,
    kind: KIND_SERVICE_BOND,
    tags,
    content: JSON.stringify({
      task: opts.taskType,
      description: opts.description,
      amount: opts.amount,
      asset: opts.asset,
      chain: "solana",
      deadline: opts.deadline,
      created_by_role: "agent",
    }),
  };
}

export function acceptance(opts: {
  agentPubkey: string;
  bondAddress: string;
  amount: string;
  witnesses: string[];
  createdAt: number;
}): UnsignedEvent {
  return {
    pubkey: opts.agentPubkey,
    created_at: opts.createdAt,
    kind: KIND_BOND_ACCEPTANCE,
    tags: [
      ["a", opts.bondAddress],
      ["p", opts.agentPubkey],
      ["amount", opts.amount],
      ...opts.witnesses.map((w) => ["witness", w] as string[]),
    ],
    content: "I bind this service bond and stake collateral.",
  };
}

export function attestation(opts: {
  witnessPubkey: string;
  bondAddress: string;
  verdict: "pass" | "fail";
  evidence?: string;
  createdAt: number;
}): UnsignedEvent {
  const tags: string[][] = [
    ["a", opts.bondAddress],
    ["p", opts.witnessPubkey],
    ["v", opts.verdict],
  ];
  if (opts.evidence) tags.push(["evidence", opts.evidence]);
  return {
    pubkey: opts.witnessPubkey,
    created_at: opts.createdAt,
    kind: KIND_PERFORMANCE_ATTESTATION,
    tags,
    content: `Attestation: ${opts.verdict}`,
  };
}

export function claim(opts: {
  claimantPubkey: string;
  bondAddress: string;
  amount: string;
  reason: string;
  createdAt: number;
}): UnsignedEvent {
  return {
    pubkey: opts.claimantPubkey,
    created_at: opts.createdAt,
    kind: KIND_BOND_CLAIM,
    tags: [
      ["a", opts.bondAddress],
      ["p", opts.claimantPubkey],
      ["amount", opts.amount],
    ],
    content: JSON.stringify({ reason: opts.reason }),
  };
}

export function verdict(opts: {
  arbitratorPubkey: string;
  bondAddress: string;
  verdict: "uphold" | "reject" | "split";
  splitRatio?: number;
  rationale: string;
  createdAt: number;
}): UnsignedEvent {
  const tags: string[][] = [
    ["a", opts.bondAddress],
    ["p", opts.arbitratorPubkey],
    ["v", opts.verdict],
  ];
  if (opts.splitRatio !== undefined) tags.push(["split", String(opts.splitRatio)]);
  return {
    pubkey: opts.arbitratorPubkey,
    created_at: opts.createdAt,
    kind: KIND_ARBITRATION_VERDICT,
    tags,
    content: JSON.stringify({ rationale: opts.rationale }),
  };
}

export function cancellation(opts: {
  agentPubkey: string;
  bondAddress: string;
  reason: string;
  createdAt: number;
}): UnsignedEvent {
  return {
    pubkey: opts.agentPubkey,
    created_at: opts.createdAt,
    kind: KIND_BOND_CANCELLATION,
    tags: [
      ["a", opts.bondAddress],
      ["p", opts.agentPubkey],
      ["reason", opts.reason],
    ],
    content: opts.reason,
  };
}

export function bondscoreLabel(opts: {
  publisherPubkey: string;
  subjectPubkey: string;
  metric: string;
  value: string;
  createdAt: number;
}): UnsignedEvent {
  return {
    pubkey: opts.publisherPubkey,
    created_at: opts.createdAt,
    kind: KIND_LABEL,
    tags: [
      ["L", "bondhive"],
      ["l", opts.metric, opts.subjectPubkey],
      ["p", opts.subjectPubkey],
    ],
    content: opts.value,
  };
}
