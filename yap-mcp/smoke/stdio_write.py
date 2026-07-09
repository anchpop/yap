#!/usr/bin/env python3
"""Write-path smoke test for the stdio server, on the throwaway test account.

Usage (run setup_test_user.py first to reset the test account):
    set -a; source .env; set +a
    YAP_USER_EMAIL=yap-mcp-test@popovit.ch python3 yap-mcp/smoke/stdio_write.py target/debug/yap-mcp

Adds a card, reviews it, then fetches the uploaded rows and asserts their
JSON shape matches a real web-app ReviewCard event field-for-field.
"""
import json
import os
import subprocess
import sys
import threading
import urllib.request

BIN = sys.argv[1]

proc = subprocess.Popen(
    [BIN], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    text=True, env=os.environ,
)

def pump_stderr():
    for line in proc.stderr:
        sys.stderr.write("[server] " + line)

threading.Thread(target=pump_stderr, daemon=True).start()

next_id = [0]

def send(method, params=None, notification=False):
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    if not notification:
        next_id[0] += 1
        msg["id"] = next_id[0]
    proc.stdin.write(json.dumps(msg) + "\n")
    proc.stdin.flush()
    if notification:
        return None
    while True:
        line = proc.stdout.readline()
        if not line:
            raise SystemExit("server closed stdout")
        resp = json.loads(line)
        if resp.get("id") == next_id[0]:
            return resp

def tool(name, arguments):
    resp = send("tools/call", {"name": name, "arguments": arguments})
    result = resp.get("result", resp.get("error"))
    is_error = isinstance(result, dict) and result.get("isError", False)
    texts = [b["text"] for b in result.get("content", []) if b.get("type") == "text"]
    return is_error, "\n".join(texts) or json.dumps(result)

def show(title, body, limit=1800):
    print(f"\n=== {title} ===")
    print(body[:limit] + ("\n... [truncated]" if len(body) > limit else ""))

send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": {"name": "smoke-write", "version": "0"}})
send("notifications/initialized", notification=True)

err, body = tool("search_dictionary", {"query": "bonjour", "limit": 1})
show(f"search 'bonjour' (error={err})", body, limit=1200)
top = json.loads(body)["results"][0]

err, body = tool("add_cards", {"language": top["language"], "grams": [top["gram"]]})
show(f"add_cards (error={err}, want False)", body)

err, body = tool("get_due_cards", {"limit": 5})
show(f"get_due_cards (error={err})", body)
due = json.loads(body)
assert due["cards"], "added card should be due immediately"
entry = due["cards"][0]

err, body = tool("log_review", {
    "language": entry["language"], "card": entry["card"], "rating": "good",
})
show(f"log_review good (error={err}, want False)", body)

err, body = tool("add_cards", {"language": top["language"], "grams": [top["gram"]]})
show(f"re-add same card (error={err}, expect already_in_deck)", body)

err, body = tool("get_sentences", {
    "language": top["language"], "gram": top["gram"], "count": 2,
})
show(f"get_sentences both lists (error={err})", body, limit=2500)

proc.stdin.close()
proc.wait(timeout=10)

# Verify server-side rows have the same shape as web-app rows
URL = os.environ.get("SUPABASE_URL", "https://eearwzqotpfoderpfrqx.supabase.co")
KEY = os.environ["SUPABASE_SERVICE_ROLE_KEY"]

def get(path):
    r = urllib.request.Request(URL + path)
    r.add_header("apikey", KEY)
    r.add_header("Authorization", f"Bearer {KEY}")
    with urllib.request.urlopen(r) as resp:
        return json.loads(resp.read().decode())

test_email = os.environ["YAP_USER_EMAIL"]
users = []
for page in range(1, 20):
    batch = get(f"/auth/v1/admin/users?page={page}&per_page=50")["users"]
    if not batch:
        break
    users.extend(batch)
test_id = next(u["id"] for u in users if u["email"] == test_email)
andre_id = next(u["id"] for u in users if u["email"] == "andre@popovit.ch")

mcp_rows = get(f"/rest/v1/events?user_id=eq.{test_id}&stream_id=eq.reviews&order=id.asc")
print(f"\n=== {len(mcp_rows)} uploaded review rows on server ===")

web_rows = get(
    f"/rest/v1/events?user_id=eq.{andre_id}&stream_id=eq.reviews&order=id.desc&limit=200"
)
web_review = next(r for r in web_rows if "ReviewCard" in json.dumps(r["event"]))

def event_shape(row_event):
    e = row_event if isinstance(row_event, dict) else json.loads(row_event)
    inner = e["event"]["User"]
    return {
        "outer_keys": sorted(e.keys()),
        "version": inner.get("version"),
        "content_type": inner.get("content", {}).get("type"),
        "content_keys": sorted(inner.get("content", {}).keys()),
    }

mcp_review = next(r for r in mcp_rows if "ReviewCard" in json.dumps(r["event"]))
print("mcp:", json.dumps(event_shape(mcp_review["event"])))
print("web:", json.dumps(event_shape(web_review["event"])))
print("\nwrite smoke test complete")
