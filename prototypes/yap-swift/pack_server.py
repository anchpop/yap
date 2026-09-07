"""Loopback fixture for Yap's chunked language-data HTTP endpoint."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from threading import Thread


@contextmanager
def serve_packs(packs):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_POST(self):
            if self.path == "/__offline":
                self.server.offline = True
                self.send_response(204)
                self.end_headers()
                return
            if self.path != "/language-data":
                self.send_error(404)
                return
            if self.server.offline:
                self.server.offline_downloads += 1
                self.send_error(503, "Fixture downloads disabled")
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 4096:
                    raise ValueError("Invalid request length")
                request = json.loads(self.rfile.read(length))
                if request["course"] != {"nativeLanguage": "English", "targetLanguage": "French"}:
                    self.send_error(404, "Fixture only supplies French for English")
                    return
                part = request["part"]
                index, size = request["chunk_index"], request["chunk_size"]
                if part not in ("core", "sentences") or type(index) is not int or type(size) is not int or index < 0 or size <= 0:
                    raise ValueError("Invalid chunk request")
            except (ValueError, KeyError, TypeError):
                self.send_error(400)
                return
            path = packs / "fra_for_eng" / f"language_data_{part}.rkyv"
            with path.open("rb") as source:
                start = index * size
                remaining = min(size, path.stat().st_size - start)
                if remaining <= 0:
                    self.send_error(416)
                    return
                source.seek(start)
                self.send_response(200)
                self.send_header("Content-Type", "application/octet-stream")
                self.send_header("Content-Length", str(remaining))
                self.end_headers()
                while remaining:
                    chunk = source.read(min(64 * 1024, remaining))
                    if not chunk:
                        raise EOFError("Fixture pack changed during download")
                    self.wfile.write(chunk)
                    remaining -= len(chunk)
            self.server.downloads.append((part, index))

    with HTTPServer(("127.0.0.1", 0), Handler) as server:
        server.url = f"http://127.0.0.1:{server.server_port}"
        server.offline = False
        server.offline_downloads = 0
        server.downloads = []
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield server
        finally:
            server.shutdown()
            thread.join()
