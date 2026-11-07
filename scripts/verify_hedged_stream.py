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
    ("adaptive-rollback-groups", "adaptive-rollback-groups", 15, True),
    ("adaptive-small", "adaptive-small", 4, True),
    ("adaptive-growth", "adaptive-growth", 4, True),
    ("adaptive-shared-limit", "adaptive-shared-limit", 4, True),
    ("adaptive-refusal", "adaptive-refusal", 4, True),
    ("adaptive-original-stop", "short-primary", 4, True),
    ("plain-zero", "plain", 0, True),
    ("plain-one", "plain", 1, True),
    ("plain-four", "plain", 4, True),
    ("slow-front-helper", "slow-front", 1, True),
    ("refused-403", "403", 1, True),
    ("refused-413", "413", 4, True),
    ("transient-503", "transient-503", 1, True),
    ("transient-429", "transient-429", 1, True),
    ("ignored-range", "ignore", 4, True),
    ("wrong-range", "wrong-range", 4, True),
    ("changed-validator", "changed", 4, True),
    ("short-helper", "short-helper", 1, True),
    ("changed-after-progress", "changed-after-progress", 1, True),
    ("refused-after-progress", "refused-after-progress", 1, True),
    ("short-primary-recovered", "short-primary", 1, True),
    ("short-primary-serial", "short-primary", 0, True),
    ("short-both", "short-both", 1, True),
    ("no-progress", "no-progress", 1, False),
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
    parser.add_argument("--cases", nargs="+", choices=[case[0] for case in CASES])
    args = parser.parse_args()
    if args.cases and len(args.cases) != len(set(args.cases)):
        raise ValueError("Use unique case labels")
    binary = args.binary.resolve()
    output = args.output.resolve()
    work = output.parent / (output.stem + "-files")
    if not binary.is_file() or output.exists() or work.exists():
        raise ValueError("Use an existing binary and fresh output/work paths")
    work.mkdir(parents=True)
    snapshot = work / "source"
    source_paths = [
        "examples/idg-hedged.rs", "examples/support/coverage.rs", "examples/support/adaptive.rs",
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
        "case_selection": args.cases or [case[0] for case in CASES],
        "cases": [],
    }

    def save():
        output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")

    save()
    for label, mode, helpers, expected_success in CASES:
        if args.cases and label not in args.cases:
            continue
        automatic = label.startswith("adaptive-")
        payload = b"" if mode == "empty" else DATA[:256 * 1024] if mode in ("slow-front", "adaptive-small") else DATA
        if mode == "adaptive-rollback-groups":
            payload = hashlib.shake_256(b"IDG abandoned helper groups fixture v1").digest(32 * 1024 * 1024)
        requests = []
        lock = threading.Lock()
        active, peak = 0, 0
        shared_next = time.monotonic()
        capacity_ready = threading.Event()
        helper_ready = threading.Event()

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
                nonlocal active, peak
                self.counted = True
                with lock:
                    active += 1
                    peak = max(active, peak)
                try:
                    self.serve_get()
                finally:
                    self.release_admission()

            def release_admission(self):
                nonlocal active
                with lock:
                    if self.counted:
                        active -= 1
                        self.counted = False

            def serve_get(self):
                nonlocal shared_next
                ranged = self.headers.get("Range")
                start, end = 0, len(payload) - 1
                if ranged:
                    start, end = map(int, ranged.removeprefix("bytes=").split("-"))
                    assert 0 <= start <= end < len(payload)
                status = 206 if ranged or mode == "partial-initial" else 200
                if ranged and mode in ("403", "413", "adaptive-refusal"):
                    status = 403 if mode == "adaptive-refusal" else int(mode)
                if ranged and mode == "ignore":
                    status, start, end = 200, 0, len(payload) - 1
                if mode == "auth" and self.headers.get("Authorization") != "Basic OnBhc3M=":
                    status = 401
                with lock:
                    if mode == "adaptive-rollback-groups" and active > 4:
                        status = 503
                    ranged_number = sum(bool(r["range"]) for r in requests) + bool(ranged)
                    if ranged:
                        helper_ready.set()
                    if ranged and mode == "refused-after-progress" and ranged_number > 1:
                        status = 403
                    if ranged and mode in ("transient-503", "transient-429") and ranged_number <= 2:
                        status = int(mode.rsplit("-", 1)[1])
                    if ranged and mode in ("transient-503", "transient-429") and ranged_number <= 2:
                        capacity_ready.set()
                    requests.append({
                        "range": ranged, "start": start, "end": end, "status": status,
                        "if_range": self.headers.get("If-Range"),
                        "authenticated": self.headers.get("Authorization") == "Basic OnBhc3M=",
                    })
                content = payload[start:end + 1] if status < 400 else b""
                changed = bool(ranged and (mode == "changed" or mode == "changed-after-progress" and ranged_number > 1))
                if changed:
                    content = bytes(value ^ 0xFF for value in content)
                if ranged and mode == "slow-front":
                    time.sleep(0.2)
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
                                     else '"changed"' if changed else '"fixed"')
                if status == 206:
                    range_start = start + (1 if ranged and mode == "wrong-range" else 0)
                    self.send_header("Content-Range", f"bytes {range_start}-{end}/{len(payload)}")
                self.end_headers()
                if not ranged and mode in ("short-primary", "short-both", "no-progress", "transient-503", "transient-429"):
                    self.wfile.write(content[:193])
                    if mode in ("transient-503", "transient-429"):
                        # Release the occupied original slot before expecting
                        # retries under the learned one-connection budget.
                        capacity_ready.wait(2)
                    return
                if ranged and (mode in ("short-helper", "short-both", "no-progress")
                               or mode in ("changed-after-progress", "refused-after-progress") and ranged_number == 1):
                    if mode != "no-progress":
                        self.wfile.write(content[:31763])
                    return
                if chunked:
                    self.wfile.write(f"{len(content):x}\r\n".encode() + content + b"\r\n0\r\n\r\n")
                elif ranged and mode not in ("adaptive-growth", "adaptive-shared-limit", "adaptive-rollback-groups"):
                    self.wfile.write(content)
                else:
                    # Give the helpers a representative opportunity to arrive.
                    if mode == "slow-front":
                        helper_ready.wait(5)
                    step = 1024 if mode == "slow-front" else 32768
                    for at in range(0, len(content), step):
                        if mode == "adaptive-shared-limit":
                            with lock:
                                due = max(time.monotonic(), shared_next)
                                shared_next = due + len(content[at:at + step]) / (1024 * 1024)
                            time.sleep(max(0, due - time.monotonic()))
                        if mode == "adaptive-rollback-groups":
                            time.sleep(0.015625)
                            if at + step >= len(content):
                                # End the logical body slot at its final write,
                                # before another request can observe a sleeping
                                # handler whose response is already complete.
                                self.release_admission()
                        self.wfile.write(content[at:at + step])
                        time.sleep(0 if mode == "adaptive-rollback-groups" else 0.03125 if mode in ("adaptive-growth", "adaptive-refusal") else 0.01 if mode == "slow-front" else 0.001)

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
                   str(destination), "--auto-helpers" if automatic else "--helpers", str(helpers)]
        try:
            process = subprocess.run(command, capture_output=True, text=True,
                                     encoding="utf-8", errors="replace", timeout=25)
        except subprocess.TimeoutExpired as error:
            def decoded(value):
                return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value or ""
            process = subprocess.CompletedProcess(command, -1, decoded(error.stdout),
                                                  decoded(error.stderr) + "\nDiagnostic timed out")
        finally:
            capacity_ready.set()
            server.shutdown()
            server.server_close()
            thread.join()
        actual = digest(destination) if destination.is_file() else None
        expected = hashlib.sha256(payload).hexdigest()
        stdout = process.stdout.strip()
        try:
            summary = json.loads(stdout) if stdout.startswith("{") else None
        except json.JSONDecodeError:
            summary = None
        metrics = summary if isinstance(summary, dict) else {}
        row = {
            "case": label, "mode": mode, "requested_helpers": helpers,
            "expected_success": expected_success, "exit_code": process.returncode,
            "automatic": automatic, "peak_active_requests": peak,
            "actual_size": destination.stat().st_size if destination.is_file() else None,
            "actual_sha256": actual, "expected_size": len(payload), "expected_sha256": expected,
            "requests": requests, "summary": summary, "stdout": process.stdout, "stderr": process.stderr,
        }
        verified = (process.returncode == 0) == expected_success
        if expected_success:
            verified &= actual == expected and row["actual_size"] == len(payload)
            verified &= metrics.get("bytes") == len(payload)
            if helpers and not automatic and mode not in ("empty", "no-etag", "weak-etag"):
                verified &= any(r["range"] for r in requests)
            if mode in ("empty", "no-etag", "weak-etag"):
                verified &= metrics.get("effective_helpers") == 0 and not any(r["range"] for r in requests)
            if mode == "auth":
                verified &= all(r["authenticated"] for r in requests)
        else:
            verified &= summary is None
        if mode == "existing":
            verified &= destination.read_bytes() == b"preserve existing user content"
        if mode in ("html", "partial-initial"):
            verified &= not destination.exists()
        if mode == "no-progress":
            verified &= sum(bool(r["range"]) for r in requests) == 22
        if mode in ("short-helper", "short-both", "changed-after-progress", "refused-after-progress") and expected_success:
            verified &= metrics.get("helper_reconnects", 0) > 0
        if mode in ("changed-after-progress", "refused-after-progress"):
            verified &= metrics.get("optional_failures") == 1
        if mode in ("transient-503", "transient-429"):
            verified &= metrics.get("capacity_retries", 0) >= 2
            verified &= sum(bool(r["range"]) for r in requests) >= 3
            verified &= any(r["range"] and r["status"] == 206 for r in requests)
        if mode == "slow-front":
            verified &= sum(bool(r["range"]) for r in requests) == 1
        if automatic:
            # A server handler can still be writing buffered bytes after the
            # client has closed a redundant response. Its peak is retained as
            # an observation, not equated with live client worker slots.
            verified &= metrics.get("automatic") is True
            verified &= metrics.get("effective_helpers", helpers + 1) <= helpers
            verified &= sum(not r["range"] for r in requests) == 1
            verified &= all(d["helpers"] <= helpers for d in metrics.get("automatic_decisions", []))
            if mode == "adaptive-rollback-groups":
                successful = [r for r in requests if r["range"] and r["status"] == 206]
                # After the original tail groups, abandoned middle work must
                # precede the low front group already approached by the original.
                later_groups = [r for r in successful[3:] if r["end"] - r["start"] + 1 > 1024 * 1024]
                verified &= bool(later_groups) and later_groups[0]["start"] >= 8 * 1024 * 1024
                verified &= any(d["action"] == "rollback" for d in metrics.get("automatic_decisions", []))
                verified &= metrics.get("trial_cancellations", 0) >= 1
            elif mode == "adaptive-small":
                verified &= not any(r["range"] for r in requests)
                verified &= metrics.get("effective_helpers") == 0
            elif mode == "adaptive-growth":
                verified &= metrics.get("effective_helpers", 0) >= 2 and peak >= 3
            elif mode == "adaptive-shared-limit":
                verified &= any(r["range"] and r["status"] == 206 for r in requests)
                verified &= any(d["action"] == "rollback" for d in metrics.get("automatic_decisions", []))
                verified &= metrics.get("trial_cancellations", 0) >= 1
            elif mode == "adaptive-refusal":
                verified &= any(r["status"] == 403 for r in requests)
                verified &= any(d["action"] == "rollback" and d["helpers"] == 0 for d in metrics.get("automatic_decisions", []))
            elif mode == "short-primary":
                verified &= metrics.get("primary_replacements") == 1
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
