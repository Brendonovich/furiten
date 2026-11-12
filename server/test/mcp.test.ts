import { beforeAll, describe, expect, test } from "bun:test";
import { type Account, account, baseUrl } from "./harness.ts";

beforeAll(() => baseUrl(), 120_000);

/** A minimal Streamable HTTP MCP client. */
const client = async (api: Account, protocolVersion: string) => {
  const url = `${await baseUrl()}/mcp`;
  let sessionId: string | null = null;
  let id = 0;
  const post = async (message: object, headers: Record<string, string> = {}) => {
    const res = await fetch(url, {
      method: "POST",
      headers: {
        authorization: `Bearer ${api.token}`,
        "content-type": "application/json",
        accept: "application/json, text/event-stream",
        "mcp-protocol-version": protocolVersion,
        ...(sessionId === null ? {} : { "mcp-session-id": sessionId }),
        ...headers,
      },
      body: JSON.stringify(message),
    });
    sessionId = res.headers.get("mcp-session-id") ?? sessionId;
    const text = await res.text();
    // Either JSON or a server-sent event stream whose last `data:` line is the response.
    const json = res.headers.get("content-type")?.includes("text/event-stream")
      ? text.split("\n").filter((l) => l.startsWith("data:")).at(-1)!.slice(5)
      : text;
    return { status: res.status, body: json === "" ? undefined : JSON.parse(json) };
  };
  const rpc = async (method: string, params?: object) => {
    const res = await post({ jsonrpc: "2.0", id: ++id, method, ...(params ? { params } : {}) });
    expect(res.status).toBe(200);
    expect(res.body.error).toBeUndefined();
    return res.body.result;
  };
  const call = async (name: string, args: object = {}) => {
    const result = await rpc("tools/call", { name, arguments: args });
    return { isError: result.isError as boolean, value: result.isError ? JSON.parse(result.content[0].text) : result.structuredContent };
  };
  return { post, rpc, call };
};

const TOOLS = [
  "list_decks",
  "create_deck",
  "search_cards",
  "create_cards",
  "update_card",
  "delete_card",
  "next_review",
  "review_card",
  "stats",
];

describe("MCP (2025-06-18, with a session)", () => {
  test("lists the contract's tools, explaining ---", async () => {
    const mcp = await client(await account(), "2025-06-18");
    const init = await mcp.rpc("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "test", version: "1" },
    });
    expect(init.serverInfo.name).toBe("Furiten");
    expect((await mcp.post({ jsonrpc: "2.0", method: "notifications/initialized" })).status).toBe(202);
    const { tools } = await mcp.rpc("tools/list");
    expect(tools.map((t: any) => t.name).sort()).toEqual([...TOOLS].sort());
    for (const name of ["create_cards", "update_card", "search_cards"]) {
      expect(tools.find((t: any) => t.name === name).description).toContain("`---`");
    }
  });

  test("tool calls drive the library", async () => {
    const api = await account();
    const mcp = await client(api, "2025-06-18");
    await mcp.rpc("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "t", version: "1" } });
    await mcp.post({ jsonrpc: "2.0", method: "notifications/initialized" });

    const deck = (await mcp.call("create_deck", { name: "Capitals" })).value;
    const created = await mcp.call("create_cards", {
      cards: [
        { deck_id: deck.id, content: "France?\n---\nParis", tags: ["#Europe"] },
        { deck_id: deck.id, content: "Japan?\n---\nTokyo" },
      ],
    });
    expect(created.value.cards.map((c: any) => c.sides)).toEqual([
      ["France?", "Paris"],
      ["Japan?", "Tokyo"],
    ]);
    const [france, japan] = created.value.cards;

    expect((await mcp.call("list_decks")).value.decks).toMatchObject([{ id: deck.id, cards: 2, new: 2 }]);
    expect((await mcp.call("search_cards", { tag: "europe" })).value.cards.map((c: any) => c.id)).toEqual([france.id]);
    expect((await mcp.call("next_review", { deck_id: deck.id })).value.card.id).toBe(france.id);
    const reviewed = await mcp.call("review_card", { id: france.id, remembered: true });
    expect(reviewed.value.schedule.state).toBe("review");
    const updated = await mcp.call("update_card", { id: japan.id, content: "Japan?\n---\nTōkyō" });
    expect(updated.value.sides).toEqual(["Japan?", "Tōkyō"]);
    expect((await mcp.call("delete_card", { id: japan.id })).value).toEqual({ ok: true });
    expect((await mcp.call("stats", { deck_id: deck.id })).value).toMatchObject({ cards: 1, reviews_today: 1 });

    // The same library through the HTTP API.
    expect((await api.get(`/v1/cards?deck_id=${deck.id}`)).body.cards.map((c: any) => c.id)).toEqual([france.id]);

    const missing = await mcp.call("update_card", { id: japan.id, content: "x" });
    expect(missing).toEqual({ isError: true, value: { code: "not_found", message: expect.any(String) } });
  });
});

describe("MCP (2026-07-28, sessionless)", () => {
  test("tools/call works without initialize", async () => {
    const mcp = await client(await account(), "2026-07-28");
    const res = await mcp.post({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: {
        name: "create_deck",
        arguments: { name: "Sessionless" },
        _meta: {
          "io.modelcontextprotocol/protocolVersion": "2026-07-28",
          "io.modelcontextprotocol/clientInfo": { name: "t", version: "1" },
          "io.modelcontextprotocol/clientCapabilities": {},
        },
      },
    }, { "mcp-method": "tools/call", "mcp-name": "create_deck" });
    expect(res).toMatchObject({ status: 200 });
    expect(res.body.result?.structuredContent?.name ?? res.body).toBe("Sessionless");
  });
});
