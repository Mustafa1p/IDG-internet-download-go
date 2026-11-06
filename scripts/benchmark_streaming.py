"""Isolate HTTP orchestration using the same IDG request/segment/writer code.

The full-GET example is diagnostic, with no segmented recovery. Downloaded
publisher artifacts are verified and never executed. Retain every trial.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import time

from benchmark_process import measure_client
from benchmark_sources import HTTPS_SOURCES, verify_sources

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--segmented", type=Path, required=True)
    parser.add_argument("--stream", type=Path, required=True)
    parser.add_argument("--curl", type=Path, default=Path(shutil.which("curl.exe") or shutil.which("curl") or "curl"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--sources", choices=HTTPS_SOURCES, nargs="+", default=["E", "F", "G"])
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--manual-connections", type=int, default=16)
    parser.add_argument("--timeout", type=float, default=120)
    args = parser.parse_args()
    output = args.output.resolve()
    work = output.parent / (output.stem + "-files")
    if (output.exists() or work.exists() or args.repeats < 1
            or not 1 <= args.manual_connections <= 32
            or not math.isfinite(args.timeout) or args.timeout <= 0
            or len(args.sources) != len(set(args.sources))):
        raise ValueError("Use fresh evidence paths, positive repeats/timeout and unique valid workloads")
    binaries = {
        "IDG-auto": args.segmented.resolve(),
        "IDG-manual": args.segmented.resolve(),
        "IDG-full": args.stream.resolve(),
        "curl-http1": args.curl.resolve(),
    }
    for path in binaries.values():
        if not path.is_file():
            raise ValueError(f"Missing diagnostic executable: {path}")
    references = verify_sources(args.sources)
    work.mkdir(parents=True)
    snapshot = work / "source"
    snapshot.mkdir()
    for relative in ["examples/idg-stream.rs", "scripts/benchmark_streaming.py"]:
        shutil.copyfile(ROOT / relative, snapshot / Path(relative).name)
    patch = subprocess.check_output(["git", "diff", "--binary"], cwd=ROOT)
    (snapshot / "working.patch").write_bytes(patch)
    source_paths = [
        "Cargo.toml", "Cargo.lock", "build.rs", "examples/idg-get.rs",
        "examples/idg-stream.rs", "scripts/benchmark_streaming.py",
        *[str(p.relative_to(ROOT)).replace("\\", "/") for p in sorted((ROOT / "src/download").glob("*.rs"))],
    ]
    result = {
        "scope": "Diagnostic controller comparison; not a new production policy. All IDG paths share request, segment and writer source.",
        "source_state": "Git HEAD plus retained new example/script and working.patch",
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "source_sha256": {name: digest(ROOT / name) for name in source_paths},
        "binary_sha256": {name: digest(path) for name, path in binaries.items()},
        "configuration": {"sources": args.sources, "repeats": args.repeats, "manual_connections": args.manual_connections,
                          "timeout": args.timeout, "http": "HTTP/1.1 for every path", "file_preallocation": "IDG paths retain production preallocation/writer behavior"},
        "publisher_references": references,
        "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "runs": [],
    }

    def save():
        output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")

    save()
    names = list(binaries)
    for key in args.sources:
        source = HTTPS_SOURCES[key]
        for repeat in range(args.repeats):
            order = names[repeat % len(names):] + names[:repeat % len(names)]
            for tool in order:
                directory = work / f"{len(result['runs']):03}-{key}-{repeat + 1}-{tool}"
                directory.mkdir()
                destination = directory / "download.bin"
                command = [str(binaries[tool]), source["url"], str(destination)]
                if tool in ["IDG-auto", "IDG-manual"]:
                    command.append("0" if tool == "IDG-auto" else str(args.manual_connections))
                elif tool == "curl-http1":
                    command = [str(binaries[tool]), "--http1.1", "--silent", "--show-error", "--fail",
                               "--location", "--output", str(destination), source["url"]]
                measurement = measure_client(command, destination, directory / "tool.log", args.timeout)
                actual_size = destination.stat().st_size if destination.is_file() else None
                actual_hash = digest(destination) if destination.is_file() else None
                verified = (measurement["exit_code"] == 0 and not measurement["sync_error"]
                            and actual_size == source["size"] and actual_hash == source["sha256"])
                row = {
                    "scenario": key, "tool": tool, "repeat": repeat + 1, "command": command,
                    "expected_size": source["size"], "expected_sha256": source["sha256"],
                    "actual_size": actual_size, "actual_sha256": actual_hash, "verified": verified,
                    "mib_per_second": source["size"] / 1048576 / measurement["durable_seconds"] if verified else None,
                    "log": str((directory / "tool.log").relative_to(output.parent)), **measurement,
                }
                result["runs"].append(row)
                save()
                print(f"{key} {repeat + 1} {tool}: {measurement['durable_seconds']:.3f}s, verified={verified}", flush=True)
                if verified:
                    destination.unlink()
    result["finished_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    save()
    if not all(row["verified"] for row in result["runs"]):
        raise AssertionError("Diagnostic failures retained; do not use failed trials as speed evidence")


if __name__ == "__main__":
    main()
