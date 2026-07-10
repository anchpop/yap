#!/usr/bin/env python3
"""Create/reset the throwaway test account used by the write-path smoke tests.

Usage:
    set -a; source .env; set +a
    python3 yap-mcp/smoke/setup_test_user.py

Creates yap-mcp-test@popovit.ch if missing, wipes its events, and copies the
source account's deck_selection events onto it so course detection works.
"""
import json
import os
import sys
import urllib.request

URL = os.environ.get("SUPABASE_URL", "https://eearwzqotpfoderpfrqx.supabase.co")
KEY = os.environ["SUPABASE_SERVICE_ROLE_KEY"]
SOURCE_EMAIL = "andre@popovit.ch"
TEST_EMAIL = "yap-mcp-test@popovit.ch"


def req(method, path, body=None, prefer=None):
    r = urllib.request.Request(URL + path, method=method)
    r.add_header("apikey", KEY)
    r.add_header("Authorization", f"Bearer {KEY}")
    r.add_header("Content-Type", "application/json")
    if prefer:
        r.add_header("Prefer", prefer)
    data = json.dumps(body).encode() if body is not None else None
    with urllib.request.urlopen(r, data) as resp:
        text = resp.read().decode()
        return json.loads(text) if text else None


def find_user(email):
    for page in range(1, 20):
        users = req("GET", f"/auth/v1/admin/users?page={page}&per_page=50")["users"]
        if not users:
            return None
        for u in users:
            if u["email"] == email:
                return u["id"]
    return None


source_id = find_user(SOURCE_EMAIL)
assert source_id, "source user not found"

test_id = find_user(TEST_EMAIL)
if test_id:
    print(f"test user exists: {test_id}")
else:
    created = req("POST", "/auth/v1/admin/users", {"email": TEST_EMAIL, "email_confirm": True})
    test_id = created["id"]
    print(f"created test user: {test_id}")

# Directory reviewers sign in with this account on yap.town, so give it a
# stable password when one is provided.
password = os.environ.get("YAP_TEST_USER_PASSWORD")
if password:
    req("PUT", f"/auth/v1/admin/users/{test_id}", {"password": password})
    print("password set from YAP_TEST_USER_PASSWORD")

# Wipe any prior test events for repeatability
req("DELETE", f"/rest/v1/events?user_id=eq.{test_id}")

rows = req(
    "GET",
    f"/rest/v1/events?user_id=eq.{source_id}&stream_id=eq.deck_selection&order=id.asc&limit=1000",
)
print(f"copying {len(rows)} deck_selection events")
payload = [
    {
        "user_id": test_id,
        "device_id": r["device_id"],
        "event": r["event"],
        "created_at": r["created_at"],
        "within_device_events_index": r["within_device_events_index"],
        "stream_id": r["stream_id"],
    }
    for r in rows
]
req("POST", "/rest/v1/events", payload, prefer="return=minimal")
print("done")
