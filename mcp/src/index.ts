#!/usr/bin/env node
/**
 * MCP server: lets an agent (a support desk, an accounts-payable bot) ask a
 * customer for proof of a shielded Zcash payment and check it, without ever
 * holding the customer's viewing key.
 */
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { QuittanceApi, describeVerdict } from "./api.js";

const base = process.env.QUITTANCE_API;
if (!base) {
  console.error("Set QUITTANCE_API to the Quittance API base URL.");
  process.exit(1);
}
const api = new QuittanceApi(base);

const server = new McpServer({ name: "quittance", version: "0.1.0" });

server.registerTool(
  "create_payment_challenge",
  {
    title: "Ask for proof of a payment",
    description:
      "Creates a fresh challenge for a customer to sign with a payment disclosure. Send them the answer_url (or the message, for the CLI). A disclosure signed over this challenge can't be replayed from anyone else's.",
    inputSchema: {
      verifier: z.string().min(1).max(200).describe("Who is asking, e.g. your company name"),
      context: z.string().min(1).max(200).describe("What the proof is for, e.g. a ticket or invoice number"),
      ttl_seconds: z.number().int().min(60).max(7 * 24 * 3600).optional(),
      network: z.enum(["main", "test"]).optional(),
    },
  },
  async (input) => {
    const t = await api.createChallenge(input);
    const text = `Challenge ${t.id} created (expires ${new Date(t.expires * 1000).toISOString()}).\nSend the customer: ${t.answer_url}\nMessage to sign: ${t.msg}`;
    return { content: [{ type: "text", text }], structuredContent: { ...t } };
  },
);

server.registerTool(
  "check_payment_challenge",
  {
    title: "Check whether a customer has answered",
    description: "Returns the status of a challenge and, once answered, the verified result.",
    inputSchema: { id: z.string().min(1).max(64) },
  },
  async ({ id }) => {
    const t = await api.getChallenge(id);
    const text = t.status === "answered" && t.verdict ? describeVerdict(t.verdict) : `Challenge ${id} is still waiting for the customer.`;
    return { content: [{ type: "text", text }], structuredContent: { ...t } };
  },
);

server.registerTool(
  "verify_payment_disclosure",
  {
    title: "Verify a payment disclosure",
    description:
      "Checks a ZIP 311 payment disclosure (text starting zpd1 or zpdtest1) against the chain. Pass the challenge you issued; without it the result can't rule out a replayed disclosure.",
    inputSchema: {
      disclosure: z.string().min(10).describe("The zpd1… / zpdtest1… text the customer sent"),
      challenge: z.string().max(1024).optional().describe("The exact challenge message you issued"),
      network: z.enum(["main", "test"]).optional().describe("The network you accept payments on; mainnet if omitted"),
    },
  },
  async ({ disclosure, challenge, network }) => {
    const v = await api.verify(disclosure.trim(), challenge, network ?? "main");
    return { content: [{ type: "text", text: describeVerdict(v) }], structuredContent: { ...v }, isError: !v.valid };
  },
);

await server.connect(new StdioServerTransport());
