"""Real-wire checks for the experimental full stream and optional helpers.

Downloads are generated nonperiodic bytes. No downloaded executable is run.
This does not exercise the desktop pause/cancel flow or change production policy.
"""
import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shutil
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
DATA = hashlib.shake_256(b"IDG full stream optional helper fixture v1").digest(8 * 1024 * 1024 + 19)
CASES = [
    ("plain-zero", "plain", 0, True),
    ("plain-one", "plain", 1, True),
    ("plain-four", "plain", 4, True),
    ("refused-403", "403", 1, True),
    ("refused-413", "413", 4, True),
    ("ignored-range", "ignore", 4, True),
    ("wrong-range", "wrong-range", 4, True),
    ("changed-validator", "changed", 4, True),
    ("short-helper", "short-helper", 1, True),
    ("short-primary-recovered", "short-primary", 1, True),
    ("short-primary-alone", "short-primary", 0, False),
    ("short-both", "short-both", 1, False),
    ("no-etag", "no-etag", 4, True),
    ("weak-etag", "weak-etag", 4, True),
    ("existing-file", "existing", 1, False),
    ("empty-file", "empty", 4, True),
    ("browser-html", "html", 1, False),
    ("partial-initial", "partial-initial", 1, False),
    ("authenticated", "auth", 1, True),
    ("chunked-helper", "chunked-helper", 1, True),
]


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    output = args.output.resolve()
    work = output.parent / (output.stem + "-files")
    if not binary.is_file() or output.exists() or work.exists():
        raise ValueError("Use an existing binary and fresh output/work paths")
    work.mkdir(parents=True)
    snapshot = work / "source"
    source_paths = [
        "examples/idg-hedged.rs", "examples/support/coverage.rs",
        "scripts/verify_hedged_stream.py", "Cargo.toml", "Cargo.lock", "build.rs",
        *[str(p.relative_to(ROOT)).replace("\\", "/")
          for p in sorted((ROOT / "src/download").glob("*.rs"))],
    ]
    for name in source_paths:
        target = snapshot / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / name, target)
    result = {
        "scope": "Diagnostic fixed-helper executable, real HTTP/1.1 and acknowledged exact output",
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "source_state": "HEAD plus retained new example/support/verifier files",
        "binary_sha256": digest(binary),
        "source_sha256": {n: digest(ROOT / n) for n in source_paths},
        "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "cases": [],
    }

    def save():
        output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")

    save()
    for label, mode, helpers, expected_success in CASES:
        payload = b"" if mode == "empty" else DATA
        requests = []
        lock = threading.Lock()

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *unused):
                pass

            def handle(self):
                try:
                    super().handle()
                except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                    self.close_connection = True

            def do_GET(self):
                ranged = self.headers.get("Range")
                start, end = 0, len(payload) - 1
                if ranged:
                    start, end = map(int, ranged.removeprefix("bytes=").split("-"))
                    assert 0 <= start <= end < len(payload)
                status = 206 if ranged or mode == "partial-initial" else 200
                if ranged and mode in ("403", "413"):
                    status = int(mode)
                if ranged and mode == "ignore":
                    status, start, end = 200, 0, len(payload) - 1
                if mode == "auth" and self.headers.get("Authorization") != "Basic OnBhc3M=":
                    status = 401
                with lock:
                    requests.append({
                        "range": ranged, "start": start, "end": end, "status": status,
                        "if_range": self.headers.get("If-Range"),
                        "authenticated": self.headers.get("Authorization") == "Basic OnBhc3M=",
                    })
                content = payload[start:end + 1] if status < 400 else b""
                if ranged and mode == "changed":
                    content = bytes(value ^ 0xFF for value in content)
                self.send_response(status)
                self.send_header("Connection", "close")
                self.send_header("Content-Type", "text/html" if mode == "html" else "application/octet-stream")
                chunked = bool(ranged and mode == "chunked-helper")
                if chunked:
                    self.send_header("Transfer-Encoding", "chunked")
                else:
                    self.send_header("Content-Length", str(len(content)))
                if mode != "no-etag":
                    self.send_header("ETag", 'W/"fixed"' if mode == "weak-etag"
                                     else '"changed"' if ranged and mode == "changed" else '"fixed"')
                if status == 206:
                    range_start = start + (1 if ranged and mode == "wrong-range" else 0)
                    self.send_header("Content-Range", f"bytes {range_start}-{end}/{len(payload)}")
                self.end_headers()
                if not ranged and mode in ("short-primary", "short-both"):
                    self.wfile.write(content[:193])
                    return
                if ranged and mode in ("short-helper", "short-both"):
                    self.wfile.write(content[:31763])
                    return
                if chunked:
                    self.wfile.write(f"{len(content):x}\r\n".encode() + content + b"\r\n0\r\n\r\n")
                elif ranged:
                    self.wfile.write(content)
                else:
                    # Give the helpers a representative opportunity to arrive.
                    for at in range(0, len(content), 32768):
                        self.wfile.write(content[at:at + 32768])
                        time.sleep(0.001)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        server.daemon_threads = True
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        directory = work / label
        directory.mkdir()
        destination = directory / "fixture.bin"
        if mode == "existing":
            destination.write_bytes(b"preserve existing user content")
        auth = ":pass@" if mode == "auth" else ""
        command = [str(binary), f"http://{auth}127.0.0.1:{server.server_port}/fixture.bin",
                   str(destination), "--helpers", str(helpers)]
        try:
            process = subprocess.run(command, capture_output=True, text=True,
                                     encoding="utf-8", errors="replace", timeout=25)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
        actual = digest(destination) if destination.is_file() else None
        expected = hashlib.sha256(payload).hexdigest()
        stdout = process.stdout.strip()
        summary = json.loads(stdout) if stdout.startswith("{") else None
        row = {
            "case": label, "mode": mode, "requested_helpers": helpers,
            "expected_success": expected_success, "exit_code": process.returncode,
            "actual_size": destination.stat().st_size if destination.is_file() else None,
            "actual_sha256": actual, "expected_size": len(payload), "expected_sha256": expected,
            "requests": requests, "summary": summary, "stdout": process.stdout, "stderr": process.stderr,
        }
        verified = (process.returncode == 0) == expected_success
        if expected_success:
            verified &= actual == expected and row["actual_size"] == len(payload)
            verified &= summary is not None and summary["bytes"] == len(payload)
            if helpers and mode not in ("empty", "no-etag", "weak-etag"):
                verified &= any(r["range"] for r in requests)
            if mode in ("empty", "no-etag", "weak-etag"):
                verified &= summary["effective_helpers"] == 0 and not any(r["range"] for r in requests)
            if mode == "auth":
                verified &= all(r["authenticated"] for r in requests)
        else:
            verified &= summary is None
        if mode == "existing":
            verified &= destination.read_bytes() == b"preserve existing user content"
        if mode in ("html", "partial-initial"):
            verified &= not destination.exists()
        row["verified"] = bool(verified)
        result["cases"].append(row)
        save()
        print(f"{label}: exit={process.returncode}, verified={verified}, "
              f"ranges={sum(bool(r['range']) for r in requests)}", flush=True)
    result["finished_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    save()
    if not all(row["verified"] for row in result["cases"]):
        raise AssertionError("Failed cases retained; do not claim this diagnostic verified")


if __name__ == "__main__":
    main()
