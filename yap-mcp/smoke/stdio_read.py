#!/usr/bin/env python3
"""Read-only smoke test for the stdio server, against a real account.

Usage (from the repo root, .env supplies the service role key):
    YAP_USER_EMAIL=you@example.com python3 yap-mcp/smoke/stdio_read.py target/debug/yap-mcp

Exercises every tool read-only and asserts the exactness guards (fake grams,
wrong language, bogus cards, bad ratings) all error. Never writes events.
"""
import copy
import json
import os
import subprocess
import sys
import threading

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
                    "clientInfo": {"name": "smoke", "version": "0"}})
send("notifications/initialized", notification=True)

err, body = tool("get_stats", {})
show(f"get_stats (error={err})", body, limit=900)

err, body = tool("get_due_cards", {"limit": 2})
show(f"get_due_cards (error={err})", body, limit=2200)
due = json.loads(body)

err, body = tool("search_dictionary", {"query": "maison", "limit": 2})
show(f"search_dictionary 'maison' (error={err})", body, limit=2500)
search = json.loads(body)
top = search["results"][0]

err, body = tool("get_sentences", {
    "language": top["language"], "gram": top["gram"], "count": 2,
})
show(f"get_sentences '{top['display_text']}' (error={err})", body, limit=2500)

# Wrong language must be rejected
err, body = tool("get_sentences", {"language": "German", "gram": top["gram"]})
show(f"get_sentences wrong language (error={err}, want True)", body)

# A fabricated gram (real word, wrong lemma) must be rejected
fake = copy.deepcopy(top["gram"])
fake[0]["Tok"]["word_type"]["lemma"] = "zzznotalemma"
err, body = tool("get_sentences", {"language": top["language"], "gram": fake})
show(f"get_sentences fake gram (error={err}, want True)", body)

err, body = tool("add_cards", {"language": top["language"], "grams": [fake]})
show(f"add_cards fake gram (error={err}, want True)", body)

# Bogus card / bad rating must be rejected
err, body = tool("log_review", {
    "language": top["language"],
    "card": {"type": "WrittenGram", "gram": fake},
    "rating": "good",
})
show(f"log_review unknown card (error={err}, want True)", body)

if due["cards"]:
    entry = due["cards"][0]
    err, body = tool("log_review", {
        "language": entry["language"], "card": entry["card"], "rating": "sideways",
    })
    show(f"log_review real card bad rating (error={err}, want True)", body)

proc.stdin.close()
proc.wait(timeout=10)
print("\nsmoke test complete")
