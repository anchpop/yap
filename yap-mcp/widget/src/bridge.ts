// The single MCP Apps bridge connection for the widget. The ext-apps App
// speaks JSON-RPC over postMessage to the host (claude.ai / ChatGPT), which
// proxies tool calls back to the yap MCP server with the user's auth.
import { App } from "@modelcontextprotocol/ext-apps";

export const app = new App(
  { name: "yap-review", version: "0.1.0" },
  {},
);

let connecting: Promise<unknown> | null = null;

/** Connect exactly once; safe to call from anywhere that needs the bridge. */
export function connectOnce(): Promise<unknown> {
  connecting ??= app.connect();
  return connecting;
}

/** First text block of a tool result, for error surfacing. */
export function resultText(result: {
  content?: Array<{ type: string; text?: string }>;
}): string {
  return (
    result.content
      ?.filter((block) => block.type === "text")
      .map((block) => block.text ?? "")
      .join("\n") ?? ""
  );
}
