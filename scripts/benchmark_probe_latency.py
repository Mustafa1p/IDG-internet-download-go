"""Compare real IDG executables on a checksummed source with fixed request latency.

This isolates metadata round trips; it is not an internet speed claim.
"""
import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import threading
import time

from benchmark_process import measure_client

CONTENT = bytes(range(256)) * 16384
EXPECTED = hashlib.sha256(CONTENT).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--latency-ms", type=float, default=80)
    args = parser.parse_args()
    if args.output.exists() or args.repeats < 1 or args.latency_ms < 0:
        raise ValueError("Use a new output file and valid repeats/latency")
    work = args.output.parent / (args.output.stem + "-files")
    work.mkdir(parents=True)
    result = dict(latency_ms=args.latency_ms, size=len(CONTENT), expected_sha256=EXPECTED,
                  binaries={name: hashlib.sha256(path.read_bytes()).hexdigest()
                            for name, path in [("baseline", args.baseline), ("candidate", args.candidate)]}, runs=[])
    for repeat in range(args.repeats):
        order = ["baseline", "candidate"] if repeat % 2 == 0 else ["candidate", "baseline"]
        for tool in order:
            requests = []
            lock = threading.Lock()

            class Handler(BaseHTTPRequestHandler):
                protocol_version = "HTTP/1.1"

                def reply(self, head=False):
                    with lock:
                        requests.append(dict(method=self.command, range=self.headers.get("Range")))
                    time.sleep(args.latency_ms / 1000)
                    start, end = 0, len(CONTENT) - 1
                    ranged = self.headers.get("Range")
                    if ranged:
                        start, end = map(int, ranged.removeprefix("bytes=").split("-"))
                    self.send_response(206 if ranged else 200)
                    self.send_header("Content-Type", "application/octet-stream")
                    self.send_header("Content-Length", str(end - start + 1))
                    self.send_header("Accept-Ranges", "bytes")
                    self.send_header("ETag", '"fixed"')
                    if ranged:
                        self.send_header("Content-Range", f"bytes {start}-{end}/{len(CONTENT)}")
                    self.end_headers()
                    if not head:
                        self.wfile.write(CONTENT[start:end + 1])

                def do_HEAD(self):
                    self.reply(head=True)

                def do_GET(self):
                    self.reply()

                def log_message(self, *args):
                    pass

            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.daemon_threads = True
            serving = threading.Thread(target=server.serve_forever, daemon=True)
            serving.start()
            directory = work / f"{repeat + 1}-{tool}"
            directory.mkdir()
            destination = directory / "download.bin"
            try:
                binary = args.baseline if tool == "baseline" else args.candidate
                command = [str(binary.resolve()), f"http://127.0.0.1:{server.server_port}/fixture.bin", str(destination.resolve()), "4"]
                measurement = measure_client(command, destination, directory / "tool.log", 30)
                actual = hashlib.sha256(destination.read_bytes()).hexdigest() if destination.exists() else None
                verified = measurement["exit_code"] == 0 and not measurement["sync_error"] and actual == EXPECTED and destination.stat().st_size == len(CONTENT)
                row = dict(tool=tool, repeat=repeat + 1, command=command, requests=requests,
                           actual_sha256=actual, verified=verified, **measurement)
                result["runs"].append(row)
                args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
                print(f"{tool} {repeat + 1}: {measurement['durable_seconds']:.3f}s, {len(requests)} requests, verified={verified}", flush=True)
            finally:
                server.shutdown()
                server.server_close()
                serving.join()
    result["finished_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
