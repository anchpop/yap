#!/usr/bin/env python3
"""Populate the throwaway test account with a believable deck + review history.

Directory reviewers (Anthropic/OpenAI) get this account's credentials; a blank
deck would make half the tools look broken. Run setup_test_user.py first to
reset the account, then this to seed it via the real stdio server (so every
event has the exact byte shape the app writes):

    set -a; source .env; set +a
    python3 yap-mcp/smoke/setup_test_user.py
    YAP_USER_EMAIL=yap-mcp-test@popovit.ch python3 yap-mcp/smoke/seed_test_user.py target/debug/yap-mcp

Leaves the account with ~15 common French cards: most reviewed once with a
mix of ratings (so stats/history look lived-in), a few never reviewed (so
get_due_cards always has something for the reviewer to grade).
"""
import json
import os
import subprocess
import sys
import threading

BIN = sys.argv[1]

# Common French words; the last few stay unreviewed so they sit in the due queue.
WORDS = [
    "bonjour", "merci", "chat", "chien", "maison", "eau", "manger", "parler",
    "rouge", "grand", "petit", "oui", "non", "ami", "livre",
]
RATINGS = ["good", "easy", "good", "hard", "good", "again", "easy", "good", "good", "hard"]

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

send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": {"name": "seed", "version": "0"}})
send("notifications/initialized", notification=True)

grams = []
for word in WORDS:
    err, body = tool("search_dictionary", {"query": word, "limit": 1})
    assert not err, f"search '{word}' failed: {body}"
    results = json.loads(body)["results"]
    if not results:
        print(f"no dictionary match for '{word}', skipping")
        continue
    top = results[0]
    grams.append((top["language"], top["gram"], top["display_text"]))

language = grams[0][0]
err, body = tool("add_cards", {"language": language, "grams": [g for _, g, _ in grams]})
assert not err, f"add_cards failed: {body}"
added = json.loads(body)
print(f"added {len(added['added'])} cards ({len(added['already_in_deck'])} already present)")

err, body = tool("get_due_cards", {"limit": len(RATINGS)})
assert not err, f"get_due_cards failed: {body}"
due = json.loads(body)["cards"]
for entry, rating in zip(due, RATINGS):
    err, body = tool("log_review", {
        "language": entry["language"], "card": entry["card"], "rating": rating,
    })
    assert not err, f"log_review failed: {body}"
    print(f"reviewed {entry.get('display_text', '?')}: {rating}")

err, body = tool("get_stats", {})
assert not err, f"get_stats failed: {body}"
print("\nfinal stats:\n" + body)

proc.stdin.close()
proc.wait(timeout=10)
print("\nseed complete")
