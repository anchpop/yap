"""Clarity-label server: static files + POST /labels/<name> persists per labeler.

POSTs are merged into the existing file (incoming keys win) rather than
overwriting it, so a fresh browser with empty localStorage can't clobber
earlier labels. Every POST is also appended verbatim to a journal
(labels-<name>.log.jsonl) for recovery.
"""
import json, re, time, http.server, socketserver

class H(http.server.SimpleHTTPRequestHandler):
    def do_POST(self):
        m = re.fullmatch(r"/labels/([a-z0-9_-]{1,32})", self.path)
        if not m:
            self.send_error(404); return
        name = m.group(1)
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        incoming = json.loads(body)  # refuse to persist non-JSON
        if not isinstance(incoming, dict):
            self.send_error(400); return
        with open(f"labels-{name}.log.jsonl", "ab") as f:
            f.write(json.dumps({"t": time.strftime("%Y-%m-%dT%H:%M:%S"),
                                "labels": incoming}).encode() + b"\n")
        try:
            with open(f"labels-{name}.json") as f:
                merged = json.load(f)
        except (FileNotFoundError, json.JSONDecodeError):
            merged = {}
        merged.update(incoming)
        with open(f"labels-{name}.json", "w") as f:
            json.dump(merged, f)
        self.send_response(204); self.end_headers()
    def log_message(self, *a): pass

socketserver.TCPServer.allow_reuse_address = True
with socketserver.TCPServer(("0.0.0.0", 8765), H) as s:
    s.serve_forever()
