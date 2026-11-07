"""Reproduce Prism A-D against the production IDG engine; retain every run.
Optional E/F/G download fixed Rust/Node/Python artifacts against publisher SHA-256 values.

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

from benchmark_process import measure_client
from benchmark_sources import HTTPS_SOURCES, verify_sources

ROOT = Path(__file__).resolve().parents[1]
MIB = 1024 * 1024
SCENARIOS = {
    "A": (1024 * MIB, 101, 16, ["--throttle", "4M"]),
    "B": (2048 * MIB, 202, 8, []),
    "C": (256 * MIB, 303, 16, ["--throttle", "2M", "--max-conn", "4", "--limit-mode", "503"]),
    "D": (256 * MIB, 404, 16, ["--throttle", "3M", "--drop-prob", "0.02"]),
}
SCENARIOS.update({key: (source["size"], 0, 16, []) for key, source in HTTPS_SOURCES.items()})


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--prism-root", type=Path, required=True)
    p.add_argument("--aria2", type=Path, required=True)
    p.add_argument("--idg", type=Path, default=ROOT / "target/release/examples/idg-get.exe")
    p.add_argument("--idg-baseline-mode", choices=["production", "http1", "http2", "no-pool", "http2-fixed"], default="production", help="Diagnostic policy for a preserved IDG CLI that supports it")
    p.add_argument("--idg-mode", choices=["production", "http1", "http2", "no-pool", "http2-fixed", "reuse-prefix"], default="production", help="Candidate CLI diagnostic policy; production is the actual desktop policy")
    p.add_argument("--idg-baseline", type=Path, help="Preserved IDG executable for interleaved before/after comparisons")
    p.add_argument("--idg-hedged", type=Path, help="Experimental full-stream helper executable; not the desktop policy")
    p.add_argument("--hedged-helpers", type=int, choices=range(32), default=1, metavar="0..31")
    p.add_argument("--idg-connections", type=int, choices=range(33), metavar="0..32", help="Diagnostic candidate override; 0 tests the app's automatic connection policy")
    p.add_argument("--idg-baseline-connections", type=int, choices=range(33), metavar="0..32", help="Diagnostic baseline override; recorded separately from the standard workload")
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--scenarios", nargs="+", choices=SCENARIOS, default=list("ABCD"))
    p.add_argument("--tools", nargs="+", choices=["curl", "aria2c", "Prism", "IDG", "IDG-baseline", "IDG-hedged", *[f"IDG-hedged-{n}" for n in range(32)]], default=["curl", "aria2c", "Prism", "IDG"])
    p.add_argument("--repeats", type=int, default=3)
    p.add_argument("--curl-repeats", type=int, help="Optional smaller curl reference sample; recorded explicitly")
    p.add_argument("--size-factor", type=float, default=1.0, help="Exploration only; published A-D use 1.0")
    p.add_argument("--trace-idg", action="store_true", help="Retain per-second progress diagnostics in candidate IDG logs")
    p.add_argument("--timeout", type=float, default=600)
    args = p.parse_args()
    hedged_counts = {tool: (args.hedged_helpers if tool == "IDG-hedged" else int(tool.rsplit("-", 1)[1]))
                     for tool in args.tools if tool == "IDG-hedged" or tool.startswith("IDG-hedged-")}
    assert args.repeats > 0 and args.size_factor > 0
    assert not any(s in HTTPS_SOURCES for s in args.scenarios) or args.size_factor == 1, "Internet file sizes are fixed"
    publisher_references = verify_sources(args.scenarios)
    prism = (args.prism_root / "target/release/examples/prism-get.exe").resolve()
    server = (args.prism_root / "target/release/prism-testserver.exe").resolve()
    binaries = {"curl": Path(shutil.which("curl.exe") or "curl"), "aria2c": args.aria2.resolve(), "Prism": prism, "IDG": args.idg.resolve()}
    if "IDG-baseline" in args.tools:
        assert args.idg_baseline, "--idg-baseline is required for IDG-baseline trials"
        binaries["IDG-baseline"] = args.idg_baseline.resolve()
    if hedged_counts:
        assert args.idg_hedged, "--idg-hedged is required for the diagnostic tool"
        for tool in hedged_counts:
            binaries[tool] = args.idg_hedged.resolve()
    for tool in args.tools:
        assert binaries[tool].is_file(), binaries[tool]
    assert server.is_file(), server
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    assert not output.exists(), "Choose a new output path; prior measurements are never overwritten"
    work = output.parent / (output.stem + "-files")
    work.mkdir()
    # Retain exact candidate sources as well as hashes when benchmarking an
    # uncommitted optimization. A hash alone cannot reconstruct those sources.
    snapshot = work / "source-snapshot"
    snapshot.mkdir()
    source_paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "examples/idg-get.rs",
                    *[ROOT / "scripts" / name for name in ("benchmark.py", "benchmark_process.py", "benchmark_sources.py", "process_metrics.py")],
                    *sorted((ROOT / "src/download").glob("*.rs"))]
    if hedged_counts:
        source_paths.extend([ROOT / "examples/idg-hedged.rs", ROOT / "examples/support/coverage.rs"])
    for source in source_paths:
        destination = snapshot / source.relative_to(ROOT)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
    (snapshot / "working.patch").write_bytes(subprocess.check_output(["git", "diff", "--binary"], cwd=ROOT))
    flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    result = {"schema": 2, "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "platform": platform.platform(), "logical_cpus": os.cpu_count(),
              "idg_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "prism_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.prism_root, text=True).strip(),
              "size_factor": args.size_factor, "repeats": args.repeats, "curl_repeats": args.curl_repeats or args.repeats,
              "timing": "Process startup through completed file and external fsync; SHA-256 is outside timed interval; OS caches are not purged",
              "binary_sha256": {k: digest(v) for k, v in binaries.items() if k in args.tools},
              "server_sha256": digest(server), "runs": []}
    result["publisher_references"] = publisher_references
    result["connection_overrides"] = {"IDG": args.idg_connections, "IDG-baseline": args.idg_baseline_connections}
    result["workloads"] = {key: dict(size=SCENARIOS[key][0], seed=SCENARIOS[key][1],
                                    requested_connections=SCENARIOS[key][2], server_options=SCENARIOS[key][3],
                                    source=HTTPS_SOURCES.get(key)) for key in args.scenarios}
    result["production_sources"] = {str(path.relative_to(ROOT)): digest(path)
        for path in sorted((ROOT / "src/download").glob("*.rs"))}
    result["driver_sha256"] = digest(ROOT / "examples/idg-get.rs")
    result["source_snapshot"] = str(snapshot.relative_to(output.parent))
    result["hedged_diagnostic"] = ({"helpers_by_tool": hedged_counts,
        "policy": "diagnostic_hedged_fixed_helpers",
        "sources": {n: digest(ROOT / n) for n in ["examples/idg-hedged.rs", "examples/support/coverage.rs"]}}
        if hedged_counts else None)
    result["harness_sha256"] = digest(ROOT / "scripts/benchmark.py")
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
        source = HTTPS_SOURCES.get(scenario)
        server_command = [str(server), "--port", str(port), "--size", str(size), "--seed", str(seed), *options] if source is None else []
        process = None
        if server_command:
            with server_log.open("wb") as log:
                process = subprocess.Popen(server_command, stdout=log, stderr=log, creationflags=flags)
        try:
            url = source["url"] if source else f"http://127.0.0.1:{port}/file/fixture.bin"
            deadline = time.monotonic() + 45
            expected = source["sha256"] if source else None
            while expected is None and time.monotonic() < deadline:
                match = re.search(r"sha256: ([0-9a-f]{64})", server_log.read_text(errors="replace"))
                if match:
                    expected = match[1]
                    break
                assert process.poll() is None, server_log.read_text(errors="replace")
                time.sleep(0.05)
            assert expected, "Server did not publish reference hash"
            destination = directory / "download.bin"
            requested_connections = 1 if tool == "curl" else connections
            if tool in ("IDG", "IDG-baseline"):
                override = args.idg_connections if tool == "IDG" else args.idg_baseline_connections
                if override is not None:
                    requested_connections = override
                command = [str(binaries[tool]), url, str(destination), str(requested_connections)]
                if tool == "IDG" and args.idg_mode != "production":
                    command.append("--" + args.idg_mode)
                if tool == "IDG-baseline" and args.idg_baseline_mode != "production":
                    command.append("--" + args.idg_baseline_mode)
            elif tool in hedged_counts:
                requested_connections = hedged_counts[tool] + 1
                command = [str(binaries[tool]), url, str(destination), "--helpers", str(hedged_counts[tool])]
            elif tool == "Prism":
                command = [str(prism), url, "-c", str(connections), "-o", str(directory)]
            elif tool == "aria2c":
                command = [str(binaries[tool]), "--disable-ipv6=true", "--console-log-level=warn", "--summary-interval=0", f"-x{connections}", f"-s{connections}", "-k1M", "--file-allocation=none", "--allow-overwrite=false", "-d", str(directory), "-o", "download.bin", url]
            else:
                command = [str(binaries[tool]), "--silent", "--show-error", "--fail", "--location", "--output", str(destination), url]
            if tool == "Prism":
                destination = directory / (url.rsplit("/", 1)[1] if source else "fixture.bin")
            measurement = measure_client(command, destination, directory / "tool.log", args.timeout,
                env={**os.environ, "IDG_TRACE_PROGRESS": "1"} if args.trace_idg and tool == "IDG" else None)
            code = measurement["exit_code"]
            process_seconds = measurement["process_seconds"]
            durable_seconds = measurement["durable_seconds"]
            sync_error = measurement["sync_error"]
            exists = destination.is_file()
            hash_started = time.perf_counter()
            actual_size = destination.stat().st_size if exists else None
            actual_hash = digest(destination) if exists else None
            verified = code == 0 and not sync_error and actual_size == size and actual_hash == expected
            row = {"scenario": scenario, "repeat": repeat, "tool": tool, "size": size, "requested_connections": requested_connections,
                   "expected_sha256": expected, "actual_sha256": actual_hash, "actual_size": actual_size,
                   "exit_code": code, "verified": verified, "process_seconds": process_seconds, "durable_seconds": durable_seconds,
                   "verification_seconds": time.perf_counter() - hash_started,
                   "mib_per_second": size / MIB / durable_seconds if verified else None,
                   "sync_error": sync_error, "command": command, "server_command": server_command,
                   "resources": measurement["resources"], "source_url": url,
                   "trace_idg": args.trace_idg and tool == "IDG",
                   "candidate_policy": args.idg_mode if tool == "IDG" else args.idg_baseline_mode if tool == "IDG-baseline" else "diagnostic_hedged_fixed_helpers" if tool in hedged_counts else None,
                   "requested_helpers": hedged_counts.get(tool),
                   "log": str((directory / "tool.log").relative_to(output.parent))}
            result["runs"].append(row)
            save()
            print(f"{scenario} repeat {repeat} {tool}: {durable_seconds:.3f}s, " + (f"{row['mib_per_second']:.2f} MiB/s SHA256 PASS" if verified else f"FAILED ({code})"), flush=True)
        finally:
            if process:
                process.terminate()
                process.wait(timeout=10)
        # Delete only this run's generated payloads, keeping logs and raw results.
        if verified:
            for file in directory.iterdir():
                if file.is_file() and file.suffix != ".log":
                    file.unlink()
    result["finished_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    save()


if __name__ == "__main__":
    main()
