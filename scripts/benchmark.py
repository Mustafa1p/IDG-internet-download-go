"""Reproduce Prism A-D against the production IDG engine; retain every run.
Optional E downloads a fixed Rust archive against its publisher's SHA-256.

Build IDG: cargo build --release --example idg-get
Build the pinned Prism checkout's prism-get example AND prism-testserver binary.
Example: python scripts/benchmark.py --prism-root PATH --aria2 PATH --output target/bench-results.json
No desktop settings, credentials or user downloads are used.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import socket
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
MIB = 1024 * 1024
SCENARIOS = {
    "A": (1024 * MIB, 101, 16, ["--throttle", "4M"]),
    "B": (2048 * MIB, 202, 8, []),
    "C": (256 * MIB, 303, 16, ["--throttle", "2M", "--max-conn", "4", "--limit-mode", "503"]),
    "D": (256 * MIB, 404, 16, ["--throttle", "3M", "--drop-prob", "0.02"]),
    "E": (79909984, 0, 16, []),
}
INTERNET_URL = "https://static.rust-lang.org/dist/rustc-1.90.0-x86_64-pc-windows-msvc.tar.xz"
INTERNET_SHA256 = "a18b3912905f154c34a7572954dda9e9849ac6411a1e02fa390bf742f918fbcb"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--prism-root", type=Path, required=True)
    p.add_argument("--aria2", type=Path, required=True)
    p.add_argument("--idg", type=Path, default=ROOT / "target/release/examples/idg-get.exe")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--scenarios", nargs="+", choices=SCENARIOS, default=list("ABCD"))
    p.add_argument("--tools", nargs="+", choices=["curl", "aria2c", "Prism", "IDG"], default=["curl", "aria2c", "Prism", "IDG"])
    p.add_argument("--repeats", type=int, default=3)
    p.add_argument("--curl-repeats", type=int, help="Optional smaller curl reference sample; recorded explicitly")
    p.add_argument("--size-factor", type=float, default=1.0, help="Exploration only; published A-D use 1.0")
    p.add_argument("--timeout", type=float, default=600)
    args = p.parse_args()
    assert args.repeats > 0 and args.size_factor > 0
    assert "E" not in args.scenarios or args.size_factor == 1, "Internet file size is fixed"
    if "E" in args.scenarios:
        with urllib.request.urlopen(INTERNET_URL + ".sha256", timeout=30) as response:
            assert response.read().decode().split()[0] == INTERNET_SHA256, "Official checksum changed"
    prism = (args.prism_root / "target/release/examples/prism-get.exe").resolve()
    server = (args.prism_root / "target/release/prism-testserver.exe").resolve()
    binaries = {"curl": Path(shutil.which("curl.exe") or "curl"), "aria2c": args.aria2.resolve(), "Prism": prism, "IDG": args.idg.resolve()}
    for tool in args.tools:
        assert binaries[tool].is_file(), binaries[tool]
    assert server.is_file(), server
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    assert not output.exists(), "Choose a new output path; prior measurements are never overwritten"
    work = output.parent / (output.stem + "-files")
    work.mkdir()
    flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    result = {"schema": 1, "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "platform": platform.platform(), "logical_cpus": os.cpu_count(),
              "idg_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "prism_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.prism_root, text=True).strip(),
              "size_factor": args.size_factor, "repeats": args.repeats, "curl_repeats": args.curl_repeats or args.repeats,
              "timing": "Process startup through completed file and external fsync; SHA-256 is outside timed interval; OS caches are not purged",
              "binary_sha256": {k: digest(v) for k, v in binaries.items() if k in args.tools},
              "server_sha256": digest(server), "runs": []}
    result["production_sources"] = {str(path.relative_to(ROOT)): digest(path)
        for path in sorted((ROOT / "src/download").glob("*.rs"))}
    result["driver_sha256"] = digest(ROOT / "examples/idg-get.rs")
    result["tracked_worktree_changes"] = subprocess.check_output(["git", "diff", "--name-only"], cwd=ROOT, text=True).splitlines()
    result["rustc"] = subprocess.check_output(["rustc", "--version"], text=True).strip()

    def save():
        temporary = output.with_suffix(".tmp")
        temporary.write_text(json.dumps(result, indent=2), encoding="utf-8")
        temporary.replace(output)

    save()
    schedule = []
    # Rotate tool order on successive repeats to reduce order/cache bias.
    for scenario in args.scenarios:
        for repeat in range(args.repeats):
            tools = args.tools[repeat % len(args.tools):] + args.tools[:repeat % len(args.tools)]
            schedule.extend((scenario, repeat + 1, tool) for tool in tools
                            if tool != "curl" or repeat < (args.curl_repeats or args.repeats))
    for index, (scenario, repeat, tool) in enumerate(schedule):
        size, seed, connections, options = SCENARIOS[scenario]
        size = int(size * args.size_factor)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        directory = work / f"{index:03}-{scenario}-{repeat}-{tool}"
        directory.mkdir()
        server_log = directory / "server.log"
        # Fresh server and destination for every tool, including each repeat.
        server_command = [str(server), "--port", str(port), "--size", str(size), "--seed", str(seed), *options] if scenario != "E" else []
        process = None
        if server_command:
            with server_log.open("wb") as log:
                process = subprocess.Popen(server_command, stdout=log, stderr=log, creationflags=flags)
        try:
            url = f"http://127.0.0.1:{port}/file/fixture.bin" if scenario != "E" else INTERNET_URL
            deadline = time.monotonic() + 45
            expected = INTERNET_SHA256 if scenario == "E" else None
            while expected is None and time.monotonic() < deadline:
                match = re.search(r"sha256: ([0-9a-f]{64})", server_log.read_text(errors="replace"))
                if match:
                    expected = match[1]
                    break
                assert process.poll() is None, server_log.read_text(errors="replace")
                time.sleep(0.05)
            assert expected, "Server did not publish reference hash"
            destination = directory / "download.bin"
            if tool == "IDG":
                command = [str(binaries[tool]), url, str(destination), str(connections)]
            elif tool == "Prism":
                command = [str(prism), url, "-c", str(connections), "-o", str(directory)]
            elif tool == "aria2c":
                command = [str(binaries[tool]), "--disable-ipv6=true", "--console-log-level=warn", "--summary-interval=0", f"-x{connections}", f"-s{connections}", "-k1M", "--file-allocation=none", "--allow-overwrite=false", "-d", str(directory), "-o", "download.bin", url]
            else:
                command = [str(binaries[tool]), "--silent", "--show-error", "--fail", "--location", "--output", str(destination), url]
            started = time.perf_counter()
            with (directory / "tool.log").open("wb") as log:
                try:
                    completed = subprocess.run(command, stdout=log, stderr=log, timeout=args.timeout, creationflags=flags)
                    code = completed.returncode
                except subprocess.TimeoutExpired:
                    code = "timeout"
            process_seconds = time.perf_counter() - started
            if tool == "Prism":
                destination = directory / ("fixture.bin" if scenario != "E" else INTERNET_URL.rsplit("/", 1)[1])
            exists = destination.is_file()
            sync_error = None
            if exists:
                try:
                    with destination.open("r+b") as file:
                        os.fsync(file.fileno())
                except OSError as error:
                    sync_error = str(error)
            durable_seconds = time.perf_counter() - started
            hash_started = time.perf_counter()
            actual_size = destination.stat().st_size if exists else None
            actual_hash = digest(destination) if exists else None
            verified = code == 0 and not sync_error and actual_size == size and actual_hash == expected
            row = {"scenario": scenario, "repeat": repeat, "tool": tool, "size": size, "requested_connections": 1 if tool == "curl" else connections,
                   "expected_sha256": expected, "actual_sha256": actual_hash, "actual_size": actual_size,
                   "exit_code": code, "verified": verified, "process_seconds": process_seconds, "durable_seconds": durable_seconds,
                   "verification_seconds": time.perf_counter() - hash_started,
                   "mib_per_second": size / MIB / durable_seconds if verified else None,
                   "sync_error": sync_error, "command": command, "server_command": server_command,
                   "log": str((directory / "tool.log").relative_to(output.parent))}
            result["runs"].append(row)
            save()
            print(f"{scenario} repeat {repeat} {tool}: {durable_seconds:.3f}s, " + (f"{row['mib_per_second']:.2f} MiB/s SHA256 PASS" if verified else f"FAILED ({code})"), flush=True)
        finally:
            if process:
                process.terminate()
                process.wait(timeout=10)
        # Delete only this run's generated payloads, keeping logs and raw results.
        for file in directory.iterdir():
            if file.is_file() and file.suffix != ".log":
                file.unlink()
    result["finished_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    save()


if __name__ == "__main__":
    main()
