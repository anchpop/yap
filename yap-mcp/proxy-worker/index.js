// mcp.yap.town -> yap-mcp.fly.dev pass-through proxy.
// Exists so users see a yap.town domain; streams SSE bodies unchanged.

// ChatGPT app domain verification (not a secret — it's published at a
// public well-known URL by design). OpenAI fetches this to prove we
// control the MCP hostname.
const OPENAI_APPS_CHALLENGE = "e2txN0HH-dWTmKd1SokjI3KkfKMjhTZxTSVLhd24c3A";

export default {
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === "/.well-known/openai-apps-challenge") {
      return new Response(OPENAI_APPS_CHALLENGE, {
        headers: { "content-type": "text/plain" },
      });
    }
    url.hostname = "yap-mcp.fly.dev";
    return fetch(new Request(url, request));
  },
};
