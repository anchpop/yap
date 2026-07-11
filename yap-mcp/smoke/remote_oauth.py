#!/usr/bin/env python3
"""End-to-end smoke test for the remote server (`yap-mcp serve`).

Usage:
    set -a; source .env; set +a
    python3 yap-mcp/smoke/remote_oauth.py target/debug/yap-mcp serve

Drives the whole claude.ai connector flow against a locally spawned server:
OAuth discovery -> dynamic client registration -> frontend approve -> PKCE code
exchange -> refresh -> authenticated streamable-HTTP MCP calls. Asserts the
security properties: PKCE enforced, codes single-use, redirect URIs pinned,
tokens MCP-scoped (raw Supabase JWTs rejected at /mcp), embedded Supabase
session encrypted. Resets the test account's password (setup_test_user.py
creates the account if needed).
"""
import base64
import hashlib
import json
import os
import secrets
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

CMD = sys.argv[1:]
PORT = 8199
BASE = f"http://localhost:{PORT}"
TEST_EMAIL = "yap-mcp-test@popovit.ch"
TEST_PASSWORD = "yap-mcp-smoke-test-pw-1"
REDIRECT_URI = "https://claude.ai/api/mcp/auth_callback"

SUPABASE_URL = os.environ.get("SUPABASE_URL", "https://eearwzqotpfoderpfrqx.supabase.co")
SERVICE_KEY = os.environ["SUPABASE_SERVICE_ROLE_KEY"]


def admin(method, path, body=None):
    r = urllib.request.Request(SUPABASE_URL + path, method=method)
    r.add_header("apikey", SERVICE_KEY)
    r.add_header("Authorization", f"Bearer {SERVICE_KEY}")
    r.add_header("Content-Type", "application/json")
    data = json.dumps(body).encode() if body is not None else None
    with urllib.request.urlopen(r, data) as resp:
        text = resp.read().decode()
        return json.loads(text) if text else None


# Give the test account a known password
users = []
for page in range(1, 20):
    batch = admin("GET", f"/auth/v1/admin/users?page={page}&per_page=50")["users"]
    if not batch:
        break
    users.extend(batch)
test_id = next(u["id"] for u in users if u["email"] == TEST_EMAIL)
admin("PUT", f"/auth/v1/admin/users/{test_id}", {"password": TEST_PASSWORD})
print(f"test user {test_id} password set")

# Refuse to run against a leftover server from a previous run
import socket
if socket.socket().connect_ex(("127.0.0.1", PORT)) == 0:
    raise SystemExit(f"port {PORT} already in use — kill the old server first")

# Start the server
env = dict(os.environ, PORT=str(PORT), YAP_MCP_BASE_URL=BASE)
proc = subprocess.Popen(CMD, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

def pump():
    for line in proc.stdout:
        sys.stderr.write("[server] " + line)

threading.Thread(target=pump, daemon=True).start()

for _ in range(50):
    try:
        with urllib.request.urlopen(f"{BASE}/health") as r:
            if r.read() == b"ok":
                break
    except Exception:
        time.sleep(0.2)
else:
    raise SystemExit("server did not come up")
print("server up")


def req(method, url, body=None, headers=None, form=False, follow=True):
    r = urllib.request.Request(url, method=method)
    for k, v in (headers or {}).items():
        r.add_header(k, v)
    data = None
    if body is not None:
        if form:
            data = urllib.parse.urlencode(body).encode()
            r.add_header("Content-Type", "application/x-www-form-urlencoded")
        else:
            data = json.dumps(body).encode()
            r.add_header("Content-Type", "application/json")
    opener = urllib.request.build_opener(NoRedirect()) if not follow else urllib.request.build_opener()
    try:
        resp = opener.open(r, data)
        return resp.status, {k.lower(): v for k, v in resp.headers.items()}, resp.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, {k.lower(): v for k, v in e.headers.items()}, e.read().decode()


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


ok = True

def check(name, cond, detail=""):
    global ok
    status = "PASS" if cond else "FAIL"
    if not cond:
        ok = False
    print(f"[{status}] {name} {detail[:400]}")


# 1. Discovery
status, _, body = req("GET", f"{BASE}/.well-known/oauth-protected-resource")
prm = json.loads(body)
check("protected-resource metadata", status == 200 and prm["authorization_servers"] == [BASE], body)

status, _, body = req("GET", f"{BASE}/.well-known/oauth-authorization-server")
asm = json.loads(body)
check("AS metadata", status == 200 and asm["code_challenge_methods_supported"] == ["S256"], body)

# 2. Unauthenticated MCP request → 401 with pointer to metadata
status, headers, body = req("POST", f"{BASE}/mcp", body={"jsonrpc": "2.0", "id": 0, "method": "ping"})
check("401 without token", status == 401 and "resource_metadata" in headers.get("www-authenticate", ""),
      headers.get("www-authenticate", ""))

# 3. Dynamic client registration
status, _, body = req("POST", asm["registration_endpoint"],
                      body={"redirect_uris": [REDIRECT_URI], "client_name": "smoke"})
reg = json.loads(body)
check("register", status == 201 and "client_id" in reg, body[:200])
client_id = reg["client_id"]

status, _, body = req("POST", asm["registration_endpoint"], body={"redirect_uris": ["javascript:alert(1)"]})
check("register rejects bad redirect", status == 400, body)

# 4. Authorize: GET form, then POST credentials
verifier = secrets.token_urlsafe(48)
challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b"=").decode()
auth_params = {
    "response_type": "code", "client_id": client_id, "redirect_uri": REDIRECT_URI,
    "state": "st4te", "code_challenge": challenge, "code_challenge_method": "S256",
}
ANON_KEY = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImVlYXJ3enFvdHBmb2RlcnBmcnF4Iiwicm9sZSI6ImFub24iLCJpYXQiOjE3NDgyMTUwOTIsImV4cCI6MjA2Mzc5MTA5Mn0.BmnDrHtD-THaSLHO9VE2X-PO6B-z9OkbxzjeIinN6b8"

def start_authorize():
    """GET /oauth/authorize -> parse the yap.town/connect redirect -> request id."""
    status, headers, _ = req("GET", f"{BASE}/oauth/authorize?" + urllib.parse.urlencode(auth_params),
                             follow=False)
    location = headers.get("location", "")
    q = urllib.parse.parse_qs(urllib.parse.urlparse(location).query)
    return status, location, q.get("request", [None])[0], q.get("mcp", [None])[0]

status, location, request_id, mcp_origin = start_authorize()
check("authorize redirects to frontend /connect",
      status in (302, 303) and "/connect?" in location, location[:140])
check("redirect carries request id + mcp origin", bool(request_id) and mcp_origin == BASE,
      f"mcp={mcp_origin}")

bad = dict(auth_params, redirect_uri="https://evil.example/cb")
status, _, body = req("GET", f"{BASE}/oauth/authorize?" + urllib.parse.urlencode(bad), follow=False)
check("authorize rejects unregistered redirect", status == 400, body)

# The user's proof: a real Supabase session for the test account
status, _, body = req("POST", f"{SUPABASE_URL}/auth/v1/token?grant_type=password",
                      headers={"apikey": ANON_KEY},
                      body={"email": TEST_EMAIL, "password": TEST_PASSWORD})
user_access_token = json.loads(body)["access_token"]

# Approving with a garbage session must fail (and burns the request id)
status, _, body = req("POST", f"{BASE}/oauth/approve",
                      body={"request_id": request_id, "access_token": "garbage"})
check("approve rejects invalid session", status == 401, body)

status, _, body = req("POST", f"{BASE}/oauth/approve",
                      body={"request_id": request_id, "access_token": user_access_token})
check("request id is single-use", status == 400, body)

def approve():
    """Fresh authorize + approve as the test user; returns the auth code."""
    _, _, request_id, _ = start_authorize()
    status, _, body = req("POST", f"{BASE}/oauth/approve",
                          body={"request_id": request_id, "access_token": user_access_token})
    resp = json.loads(body)
    return status, resp

status, resp = approve()
check("approve returns claude redirect",
      status == 200 and resp.get("redirect", "").startswith(REDIRECT_URI),
      json.dumps(resp)[:140])
q = urllib.parse.parse_qs(urllib.parse.urlparse(resp["redirect"]).query)
code = q["code"][0]
check("state round-trips", q.get("state") == ["st4te"])

# 5. Token exchange (PKCE)
status, _, body = req("POST", asm["token_endpoint"], form=True, body={
    "grant_type": "authorization_code", "code": code, "redirect_uri": REDIRECT_URI,
    "client_id": client_id, "code_verifier": "wrong-verifier",
})
check("wrong PKCE verifier rejected", status == 400, body)

# The code was consumed by the failed attempt (single use) — approve again
_, resp = approve()
code = urllib.parse.parse_qs(urllib.parse.urlparse(resp["redirect"]).query)["code"][0]

status, _, body = req("POST", asm["token_endpoint"], form=True, body={
    "grant_type": "authorization_code", "code": code, "redirect_uri": REDIRECT_URI,
    "client_id": client_id, "code_verifier": verifier,
})
tokens = json.loads(body)
check("token exchange", status == 200 and "access_token" in tokens and "refresh_token" in tokens,
      body[:120])

status, _, body = req("POST", asm["token_endpoint"], form=True, body={
    "grant_type": "authorization_code", "code": code, "redirect_uri": REDIRECT_URI,
    "client_id": client_id, "code_verifier": verifier,
})
check("code is single-use", status == 400, body)

# 6. Refresh grant
status, _, body = req("POST", asm["token_endpoint"], form=True, body={
    "grant_type": "refresh_token", "refresh_token": tokens["refresh_token"],
})
refreshed = json.loads(body)
check("refresh grant", status == 200 and "access_token" in refreshed, body[:120])
access = refreshed["access_token"]

# 6b. Tokens must be MCP-scoped, not raw Supabase credentials
def jwt_payload(token):
    part = token.split(".")[1]
    return json.loads(base64.urlsafe_b64decode(part + "=" * (-len(part) % 4)))

payload = jwt_payload(refreshed["access_token"])
check("access token aud is yap-mcp", payload.get("aud") == "yap-mcp", json.dumps(payload)[:200])
check("embedded supabase token is encrypted", "." not in payload.get("sb", "."), payload.get("sb", "")[:40])
check("refresh token aud is yap-mcp-refresh",
      jwt_payload(refreshed["refresh_token"]).get("aud") == "yap-mcp-refresh")

# A raw Supabase user JWT must NOT work against /mcp
anon_key = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImVlYXJ3enFvdHBmb2RlcnBmcnF4Iiwicm9sZSI6ImFub24iLCJpYXQiOjE3NDgyMTUwOTIsImV4cCI6MjA2Mzc5MTA5Mn0.BmnDrHtD-THaSLHO9VE2X-PO6B-z9OkbxzjeIinN6b8"
status, _, body = req("POST", f"{SUPABASE_URL}/auth/v1/token?grant_type=password",
                      headers={"apikey": anon_key},
                      body={"email": TEST_EMAIL, "password": TEST_PASSWORD})
raw_supabase_token = json.loads(body)["access_token"]
status, _, _ = req("POST", f"{BASE}/mcp",
                   headers={"Authorization": f"Bearer {raw_supabase_token}",
                            "Accept": "application/json, text/event-stream"},
                   body={"jsonrpc": "2.0", "id": 99, "method": "ping"})
check("raw Supabase JWT rejected at /mcp", status == 401, str(status))

# 7. MCP over streamable HTTP
def sse_json(text):
    for line in text.splitlines():
        if line.startswith("data:"):
            payload = line[5:].strip()
            try:
                msg = json.loads(payload)
            except json.JSONDecodeError:
                continue  # SSE priming/retry events aren't JSON-RPC
            if isinstance(msg, dict) and msg.get("jsonrpc") == "2.0":
                return msg
    return json.loads(text)  # json_response mode fallback

mcp_headers = {
    "Authorization": f"Bearer {access}",
    "Accept": "application/json, text/event-stream",
}
status, headers, body = req("POST", f"{BASE}/mcp", headers=mcp_headers, body={
    "jsonrpc": "2.0", "id": 1, "method": "initialize",
    "params": {"protocolVersion": "2025-06-18", "capabilities": {},
               "clientInfo": {"name": "smoke", "version": "0"}},
})
init = sse_json(body)
session_id = headers.get("mcp-session-id")
check("mcp initialize", status == 200 and "serverInfo" in init.get("result", {}), body[:200])
# Stateless streamable HTTP: the server issues no session id, so a deploy
# that replaces the machine can't strand a client on a stale session. Every
# request stands alone on its bearer token; there's no Mcp-Session-Id to echo.
check("no session id issued (stateless)", not session_id, str(session_id))

status, _, body = req("POST", f"{BASE}/mcp", headers=mcp_headers,
                      body={"jsonrpc": "2.0", "method": "notifications/initialized"})
check("initialized notification", status in (200, 202), body[:100])

status, _, body = req("POST", f"{BASE}/mcp", headers=mcp_headers,
                      body={"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
tools = sse_json(body)
names = sorted(t["name"] for t in tools["result"]["tools"])
check("tools/list", "get_stats" in names, str(names))

status, _, body = req("POST", f"{BASE}/mcp", headers=mcp_headers, body={
    "jsonrpc": "2.0", "id": 3, "method": "tools/call",
    "params": {"name": "get_stats", "arguments": {}},
})
stats_resp = sse_json(body)
stats_text = stats_resp["result"]["content"][0]["text"]
stats = json.loads(stats_text)
check("get_stats via remote", "French" in stats["course"], stats["course"])
print("\n--- get_stats over OAuth'd remote MCP ---")
print(stats_text[:600])

# 8. Search + review round trip as the user (RLS path, no service key)
status, _, body = req("POST", f"{BASE}/mcp", headers=mcp_headers, body={
    "jsonrpc": "2.0", "id": 4, "method": "tools/call",
    "params": {"name": "search_dictionary", "arguments": {"query": "merci", "limit": 1}},
})
search = json.loads(sse_json(body)["result"]["content"][0]["text"])
top = search["results"][0]
check("remote search", top["display_text"] == "merci", top["display_text"])

status, _, body = req("POST", f"{BASE}/mcp", headers=mcp_headers, body={
    "jsonrpc": "2.0", "id": 5, "method": "tools/call",
    "params": {"name": "add_cards",
               "arguments": {"language": top["language"], "grams": [top["gram"]]}},
})
added = json.loads(sse_json(body)["result"]["content"][0]["text"])
check("remote add_cards (RLS write)", "merci" in added["added"] + added["already_in_deck"],
      json.dumps(added))

proc.terminate()
proc.wait(timeout=5)
print("\nremote smoke test:", "ALL PASS" if ok else "FAILURES ABOVE")
sys.exit(0 if ok else 1)
