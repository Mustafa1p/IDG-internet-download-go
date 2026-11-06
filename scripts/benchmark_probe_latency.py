"""Compare real IDG executables on a checksummed source with fixed request latency.

This isolates metadata round trips; it is not an internet speed claim.
"""
import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
from pathlib import Path
import threading
import time

from benchmark_process import measure_client



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--latency-ms", type=float, default=80)
    parser.add_argument("--size-kib", type=int, default=4096)
    parser.add_argument("--connections", type=int, default=4)
    parser.add_argument("--candidate-policy", choices=["production", "reuse-prefix"], default="production")
    parser.add_argument("--per-connection-mib", type=float, default=0)
    parser.add_argument("--first-range-mib", type=float, default=0)
    args = parser.parse_args()
    rates = (args.latency_ms, args.per_connection_mib, args.first_range_mib)
    if (args.output.exists() or args.repeats < 1 or args.size_kib < 1
            or not 0 <= args.connections <= 32
            or any(not math.isfinite(value) or value < 0 for value in rates)):
        raise ValueError("Use a new output file, positive repeats/size, 0..32 connections and finite nonnegative delays/rates")
    CONTENT = bytes(range(256)) * (args.size_kib * 4)
    EXPECTED = hashlib.sha256(CONTENT).hexdigest()
    work = args.output.parent / (args.output.stem + "-files")
    work.mkdir(parents=True)
    result = dict(configuration={key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()},
                  fixture_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  latency_ms=args.latency_ms, size=len(CONTENT), expected_sha256=EXPECTED,
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
                        end = min(end, len(CONTENT) - 1)
                    self.send_response(206 if ranged else 200)
                    self.send_header("Content-Type", "application/octet-stream")
                    self.send_header("Content-Length", str(end - start + 1))
                    self.send_header("Accept-Ranges", "bytes")
                    self.send_header("ETag", '"fixed"')
                    if ranged:
                        self.send_header("Content-Range", f"bytes {start}-{end}/{len(CONTENT)}")
                    self.end_headers()
                    if not head:
                        rate = args.first_range_mib if args.first_range_mib and start == 0 and end > 0 else args.per_connection_mib
                        if rate:
                            for offset in range(start, end + 1, 32768):
                                data = CONTENT[offset:min(offset + 32768, end + 1)]
                                time.sleep(len(data) / (rate * 1024 * 1024))
                                self.wfile.write(data)
                        else:
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
                command = [str(binary.resolve()), f"http://127.0.0.1:{server.server_port}/fixture.bin", str(destination.resolve()), str(args.connections)]
                if tool == "candidate" and args.candidate_policy != "production":
                    command.append("--" + args.candidate_policy)
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
