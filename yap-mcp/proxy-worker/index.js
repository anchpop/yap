// mcp.yap.town -> yap-mcp.fly.dev pass-through proxy.
// Exists so users see a yap.town domain; streams SSE bodies unchanged.
export default {
  async fetch(request) {
    const url = new URL(request.url);
    url.hostname = "yap-mcp.fly.dev";
    return fetch(new Request(url, request));
  },
};
