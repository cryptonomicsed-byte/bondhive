/**
 * Bondhive MCP server — the agent surface of the reliability layer.
 *
 * Every protocol capability is exposed as a discoverable, invocable tool so
 * any agent (in Buzz or elsewhere) can create bonds, attest, stake, trigger
 * buy-backs and read BondScores. Agents discover this server via NIP-89
 * handler announcements (kind 31990, see spec/NIP-74.md).
 *
 * Run:  npm run build && npm start   (stdio transport — plug into any MCP host)
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import * as econ from "./economics.js";
import * as events from "./events.js";

const server = new McpServer({
  name: "bondhive",
  version: "0.1.0",
});

type TextResult = { content: { type: "text"; text: string }[] };

/** Local wrapper matching the SDK's registerTool(name, config, cb) signature. */
function tool<Shape extends z.ZodRawShape>(
  name: string,
  description: string,
  inputSchema: Shape,
  cb: (args: z.infer<z.ZodObject<Shape>>) => TextResult | Promise<TextResult>
) {
  const handler = async (args: unknown) => cb(args as z.infer<z.ZodObject<Shape>>);
  server.registerTool(name, { description, inputSchema }, handler as never);
}

tool(
  "bond.create",
  "Create a NIP-74 Service Bond (kind 37000): an agent's promise, backed by collateral. Returns an unsigned event ready for signing (NIP-46) and publishing.",
  {
    agentPubkey: z.string().describe("Hex pubkey of the promising agent"),
    channel: z.string().describe("Buzz channel id (NIP-29 h tag) where the promise lives"),
    witnesses: z.array(z.string()).describe("Witness pubkeys who will verify the work"),
    taskType: z.string().describe("e.g. code-review, research, infra, content"),
    bondId: z.string().describe("32-hex bond id (task hash)"),
    amount: z.string().describe("Collateral amount in base units (BOND 9dp)"),
    asset: z.string().default("BOND").describe("BOND | SOL | USDC"),
    deadline: z.number().describe("Unix deadline"),
    description: z.string().describe("Task description"),
    createdAt: z.number().describe("Unix timestamp"),
  },
  (args) => ({ content: [{ type: "text", text: JSON.stringify(events.serviceBond(args), null, 2) }] })
);

tool(
  "bond.quote",
  "Quote the effective premium for a bond given the agent's BondScore (discount curve: 60→10%, 70→25%, 80→50%).",
  {
    basePremium: z.number().describe("Base premium in BOND base units (e.g. 300 for 3% of 10,000)"),
    score: z.number().min(0).max(100).describe("Agent BondScore 0-100"),
  },
  ({ basePremium, score }) => {
    const discount = econ.premiumDiscount(score);
    const effective = econ.effectivePremium(basePremium, score);
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify({ basePremium, score, discountPct: discount, effectivePremium: effective }, null, 2),
        },
      ],
    };
  }
);

tool(
  "bond.attest",
  "Record a NIP-74 Performance Attestation (kind 37002). Only witnesses on the bond roster should attest.",
  {
    witnessPubkey: z.string(),
    bondAddress: z.string().describe("37000:<agent>:<bondId>"),
    verdict: z.enum(["pass", "fail"]),
    evidence: z.string().optional().describe("Artifact reference (hash or URL)"),
    createdAt: z.number(),
  },
  (args) => ({ content: [{ type: "text", text: JSON.stringify(events.attestation(args), null, 2) }] })
);

tool(
  "bond.claim",
  "File a NIP-74 dispute claim (kind 37003, gift-wrapped via NIP-59 in transport).",
  {
    claimantPubkey: z.string(),
    bondAddress: z.string(),
    amount: z.string(),
    reason: z.string(),
    createdAt: z.number(),
  },
  (args) => ({ content: [{ type: "text", text: JSON.stringify(events.claim(args), null, 2) }] })
);

tool(
  "bond.resolve.preview",
  "Preview the economic outcome of resolving a bond: release (pass) or the exact slash split (fail).",
  {
    amount: z.number().describe("Collateral in base units"),
    attestations: z.number(),
    passes: z.number(),
    quorum: z.number().default(3),
  },
  ({ amount, attestations, passes, quorum }) => {
    const pass = econ.resolvesPass(attestations, passes, quorum);
    const split = pass ? null : econ.slashSplit(amount);
    const fee = econ.witnessFee(amount);
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify({ resolvesPass: pass, quorum, witnessFee: fee, slashSplit: split }, null, 2),
        },
      ],
    };
  }
);

tool(
  "pool.stake",
  "Stake BOND into the mutual pool. Returns the payload for the Solana program Stake instruction.",
  {
    stakerPubkey: z.string(),
    amount: z.number().describe("BOND base units"),
  },
  ({ stakerPubkey, amount }) => ({
    content: [
      {
        type: "text",
        text: JSON.stringify({ instruction: "Stake", program: "bondhive", stakerPubkey, amount }, null, 2),
      },
    ],
  })
);

tool(
  "pool.status",
  "Current protocol pool state (reads on-chain accounting).",
  {
    treasuryBond: z.number(),
    claimsPool: z.number(),
    escrow: z.number(),
    stakedTotal: z.number(),
    burnedTotal: z.number(),
    milestoneVolume: z.number(),
    milestoneNext: z.number(),
    bondsActive: z.number(),
  },
  (pool) => {
    const milestone = econ.milestoneBurn(pool.milestoneVolume, pool.milestoneNext, pool.treasuryBond);
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify(
            {
              ...pool,
              nextMilestoneBurn: milestone,
              milestoneProgressPct: Math.min(100, Math.round((pool.milestoneVolume / pool.milestoneNext) * 100)),
            },
            null,
            2
          ),
        },
      ],
    };
  }
);

tool(
  "buyback.trigger",
  "Preview a treasury buy-back: 50% of the purchased BOND is burned, 50% redistributed to stakers pro-rata.",
  {
    amount: z.number().describe("BOND base units the keeper will buy on the DEX"),
    treasuryBond: z.number(),
  },
  ({ amount, treasuryBond }) => {
    if (amount > treasuryBond) {
      return { content: [{ type: "text", text: "ERROR: amount exceeds treasury balance" }] };
    }
    const split = econ.buybackSplit(amount);
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify(
            {
              amount,
              burned: split.burn,
              redistributedToStakers: split.redistribute,
              remainingTreasury: treasuryBond - amount,
            },
            null,
            2
          ),
        },
      ],
    };
  }
);

tool(
  "score.lookup",
  "Look up a BondScore and outcome counts for a pubkey (reads NIP-32 labels in the bondhive namespace).",
  {
    pubkey: z.string(),
    score: z.number().min(0).max(100),
    bondcount: z.number().default(0),
    bondsuccess: z.number().default(0),
    bondfail: z.number().default(0),
    bondfraud: z.number().default(0),
  },
  (args) => {
    const { pubkey, score, bondcount, bondsuccess, bondfail, bondfraud } = args;
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify(
            {
              pubkey,
              score,
              counts: { bondcount, bondsuccess, bondfail, bondfraud },
              premiumDiscountPct: econ.premiumDiscount(score),
              tier: score >= 80 ? "sovereign" : score >= 60 ? "trusted" : score >= 40 ? "emerging" : "newborn",
            },
            null,
            2
          ),
        },
      ],
    };
  }
);

tool(
  "announce.handler",
  "Publish the NIP-89 handler announcement (kind 31990) advertising that this app handles NIP-74 kinds.",
  {
    appPubkey: z.string(),
    name: z.string().default("Bondhive Oracle"),
    url: z.string().default("nostr+https://bondhive.dev"),
    createdAt: z.number(),
  },
  ({ appPubkey, name, url, createdAt }) => ({
    content: [
      {
        type: "text",
        text: JSON.stringify(
          {
            pubkey: appPubkey,
            created_at: createdAt,
            kind: events.KIND_HANDLER_ANNOUNCEMENT,
            tags: [
              ["d", "bondhive-oracle"],
              ["name", name],
              ["url", url],
              ["k", String(events.KIND_SERVICE_BOND)],
              ["k", String(events.KIND_PERFORMANCE_ATTESTATION)],
              ["k", String(events.KIND_BOND_ACCEPTANCE)],
            ],
            content: "",
          },
          null,
          2
        ),
      },
    ],
  })
);

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error("Bondhive MCP server ready on stdio");
}

main().catch((err) => {
  console.error("fatal:", err);
  process.exit(1);
});
