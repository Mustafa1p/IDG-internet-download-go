"""Create the repository's benchmark tables/chart from retained raw trials."""
import argparse
import json
import math
from pathlib import Path
import re
import shutil
import statistics

from benchmark_sources import HTTPS_SOURCES

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ["curl", "aria2c", "Prism", "IDG"]
LABELS = {
    "A": "1 GiB, 4 MiB/s per connection",
    "B": "2 GiB, unlimited localhost",
    "C": "256 MiB, four connections at 2 MiB/s",
    "D": "256 MiB, 2% random drops per chunk",
    "E": "76.2 MiB public Rust archive over HTTPS",
}
LABELS.update({key: source["label"] for key, source in HTTPS_SOURCES.items()})
CHART_LABELS = {
    "A": "Per-connection throttle", "B": "Unlimited localhost",
    "C": "Four-connection server", "D": "Random connection drops",
    "E": "Public HTTPS archive",
    "F": "Node HTTPS ZIP", "G": "Python HTTPS installer",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def summarize(runs, *, tools=TOOLS, scenarios=LABELS, repeats=3, curl_repeats=1):
    output = {}
    require(repeats > 0 and curl_repeats > 0, "Positive repeat counts required")
    require(runs and all(r["tool"] in tools and r["scenario"] in scenarios for r in runs), "Unexpected tool/scenario")
    for scenario in scenarios:
        output[scenario] = {}
        references = {(r["size"], r["expected_sha256"]) for r in runs if r["scenario"] == scenario}
        require(len(references) == 1, f"Missing or mixed source files in {scenario}")
        for tool in tools:
            rows = [r for r in runs if r["scenario"] == scenario and r["tool"] == tool]
            expected = min(curl_repeats, repeats) if tool == "curl" else repeats
            require(len(rows) == expected, f"{scenario} {tool}: {len(rows)} trials, expected {expected}")
            require(sorted(r["repeat"] for r in rows) == list(range(1, expected + 1)), "Missing or duplicate repeats")
            policies = {r.get("candidate_policy") for r in rows}
            require(len(policies) == 1, "Cannot combine different transport policies")
            require(len({r.get("requested_connections") for r in rows}) == 1, "Cannot combine different connection settings")
            for r in rows:
                require(type(r["verified"]) is bool, "Verification must be a boolean")
                require(math.isfinite(r["durable_seconds"]) and r["durable_seconds"] > 0, "Invalid duration")
                require(r["size"] > 0 and math.isfinite(r["size"]), "Invalid file size")
                if r["verified"]:
                    require(re.fullmatch("[0-9a-f]{64}", r["expected_sha256"]), "Missing publisher/server hash")
                    require(r["expected_sha256"] == r["actual_sha256"], "Verified hash mismatch")
                    require(r["actual_size"] == r["size"], "Verified size mismatch")
                    require(r["exit_code"] == 0 and not r["sync_error"], "Verified transfer failed")
                    require(abs(r["mib_per_second"] - r["size"] / 1048576 / r["durable_seconds"]) < 1e-8, "Incorrect throughput")
            successful = [r for r in rows if r["verified"]]
            rates = [r["mib_per_second"] for r in successful]
            resources = []
            cycles = []
            for row in rows:
                usage = row.get("resources", {})
                cycle_count = usage.get("cpu_cycles")
                if cycle_count is not None:
                    require(type(cycle_count) is int and 0 <= cycle_count <= (1 << 64) - 1, "Invalid process cycle counter")
                    cycles.append(cycle_count)
                fields = ("cpu_total_seconds", "peak_working_set_bytes", "peak_commit_bytes")
                if usage.get("errors") or not all(key in usage for key in fields):
                    continue
                require(all(math.isfinite(usage[key]) and usage[key] >= 0 for key in fields), "Invalid resource counter")
                resources.append(usage)
            output[scenario][tool] = {
                "attempts": len(rows), "verified": len(successful),
                "median_mib_s": statistics.median(rates) if rates else None,
                "min_mib_s": min(rates) if rates else None,
                "max_mib_s": max(rates) if rates else None,
                "median_seconds": statistics.median(r["durable_seconds"] for r in successful) if rates else None,
                "all_verified": len(successful) == expected,
                "resource_samples": len(resources),
                "cycle_samples": len(cycles),
                "median_client_cpu_cycles": statistics.median(cycles) if cycles else None,
                "median_client_cpu_seconds": statistics.median(r["cpu_total_seconds"] for r in resources) if resources else None,
                "max_client_peak_working_set_mib": max(r["peak_working_set_bytes"] for r in resources) / 1048576 if resources else None,
                "max_client_peak_commit_mib": max(r["peak_commit_bytes"] for r in resources) / 1048576 if resources else None,
            }
    return output


def cell(s):
    if not s["verified"]:
        return f"Failed ({s['attempts']}/{s['attempts']})"
    value = f"{s['median_mib_s']:,.2f} MiB/s"
    if s["verified"] != s["attempts"]:
        value += f" ({s['verified']}/{s['attempts']} verified)"
    return value


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("results", type=Path)
    p.add_argument("--output", type=Path, default=ROOT / "docs/benchmarks")
    p.add_argument("--scenarios", nargs="+", choices=LABELS, help="Expected complete workload set; rejects missing scenarios")
    p.add_argument("--idg-label", default="IDG", help="Display label for the measured candidate; no inferred release version")
    p.add_argument("--no-chart", action="store_true", help="Generate raw data and tables without matplotlib")
    args = p.parse_args()
    source = json.loads(args.results.read_text(encoding="utf-8"))
    require(source["size_factor"] == 1 and source.get("finished_utc"), "Only completed full-size measurements can be reported")
    tools = list(source["binary_sha256"])
    scenarios = args.scenarios or list(source.get("workloads", {})) or [s for s in LABELS if any(r["scenario"] == s for r in source["runs"])]
    require(len(tools) == len(set(tools)) and len(scenarios) == len(set(scenarios)), "Duplicate tool/scenario")
    summary = summarize(source["runs"], tools=tools, scenarios=scenarios,
                        repeats=source["repeats"], curl_repeats=source.get("curl_repeats", source["repeats"]))
    # Keep older reports immutable, including on validation failure.
    args.output.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(args.results, args.output / "results.json")
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    # Retain the complete per-trial diagnostic logs alongside the raw numbers.
    for r in source["runs"]:
        log = args.results.parent / r["log"]
        target = args.output / r["log"]
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(log, target)
        server_log = log.with_name("server.log")
        if server_log.exists():
            shutil.copyfile(server_log, target.with_name("server.log"))

    labels = {tool: args.idg_label if tool == "IDG" else tool for tool in tools}
    for tool, count in source.get("connection_overrides", {}).items():
        if tool in labels and count is not None:
            labels[tool] += " (automatic connections)" if count == 0 else f" ({count} requested connection{'s' if count != 1 else ''})"
    counts = {tool: min(source.get("curl_repeats", source["repeats"]), source["repeats"]) if tool == "curl" else source["repeats"] for tool in tools}
    lines = ["| Scenario | " + " | ".join(f"{labels[t]} (n={counts[t]})" for t in tools) + " |",
             "| --- | " + " | ".join("---:" for _ in tools) + " |"]
    for scenario in scenarios:
        label = LABELS[scenario]
        lines.append("| " + label + " | " + " | ".join(cell(summary[scenario][tool]) for tool in tools) + " |")
    table = "\n".join(lines)
    (args.output / "table.md").write_text(table + "\n", encoding="utf-8")
    detail = ["| Scenario | Tool | Verified / attempted | Median seconds | Median MiB/s | Observed min–max MiB/s |",
              "| --- | --- | ---: | ---: | ---: | ---: |"]
    for scenario in scenarios:
        for tool in tools:
            s = summary[scenario][tool]
            time = f"{s['median_seconds']:.3f}" if s["verified"] else "—"
            median = f"{s['median_mib_s']:.2f}" if s["verified"] else "—"
            spread = f"{s['min_mib_s']:.2f}–{s['max_mib_s']:.2f}" if s["verified"] else "—"
            detail.append(f"| {scenario} | {tool} | {s['verified']} / {s['attempts']} | {time} | {median} | {spread} |")
    (args.output / "detail.md").write_text("\n".join(detail) + "\n", encoding="utf-8")
    if any(s["resource_samples"] or s["cycle_samples"] for group in summary.values() for s in group.values()):
        resource_lines = ["Client process only; excludes descendants and the test server. Includes measured failed attempts. Missing counters are not zeroes.", "",
                          "| Scenario | Tool | Measured / attempted | Median client CPU seconds | Maximum client peak working set MiB | Maximum client peak commit MiB |",
                          "| --- | --- | ---: | ---: | ---: | ---: |"]
        for scenario in scenarios:
            for tool in tools:
                s = summary[scenario][tool]
                values = [s[k] for k in ("median_client_cpu_seconds", "max_client_peak_working_set_mib", "max_client_peak_commit_mib")]
                resource_lines.append(f"| {scenario} | {tool} | {s['resource_samples']} / {s['attempts']} | " + " | ".join(f"{v:.3f}" if v is not None else "—" for v in values) + " |")
        if any(s["cycle_samples"] for group in summary.values() for s in group.values()):
            resource_lines += ["", "Interpret CPU-time medians with the full trial distribution. CPU cycles are independent hardware-dependent counts, not elapsed time; no conversion to seconds is made.", "",
                               "| Scenario | Tool | Cycle samples / attempted | Median client CPU cycles (millions) |",
                               "| --- | --- | ---: | ---: |"]
            for scenario in scenarios:
                for tool in tools:
                    s = summary[scenario][tool]
                    cycles = s["median_client_cpu_cycles"]
                    value = f"{cycles / 1_000_000:.3f}" if cycles is not None else "—"
                    resource_lines.append(f"| {scenario} | {tool} | {s['cycle_samples']} / {s['attempts']} | {value} |")
        (args.output / "resources.md").write_text("\n".join(resource_lines) + "\n", encoding="utf-8")
    identity = {key: source.get(key) for key in ("idg_commit", "prism_commit", "binary_sha256", "production_sources", "harness_sha256", "started_utc", "finished_utc", "source_snapshot", "tracked_worktree_changes", "publisher_references", "workloads", "connection_overrides")}
    (args.output / "identity.json").write_text(json.dumps(identity, indent=2) + "\n", encoding="utf-8")
    if source.get("source_snapshot"):
        shutil.copytree(args.results.parent / source["source_snapshot"], args.output / source["source_snapshot"])
    print(table)
    if args.no_chart or "IDG" not in tools or "Prism" not in tools:
        return

    # A static figure is appropriate for the requested GitHub Markdown surface.
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 11})
    fig, ax = plt.subplots(figsize=(11.2, max(3.6, 2.2 + len(scenarios) * 0.7)), dpi=160)
    ratios = []
    for scenario in scenarios:
        i, p = summary[scenario]["IDG"], summary[scenario]["Prism"]
        ratios.append(i["median_mib_s"] / p["median_mib_s"] if i["all_verified"] and p["all_verified"] else None)
    chart_labels = [f"{s}  {CHART_LABELS[s]}" for s in scenarios]
    for y, ratio in enumerate(ratios):
        if ratio is None:
            ax.text(0.04, y, "Not comparable: at least one failed trial", va="center", fontsize=10)
        else:
            ax.barh(y, ratio, color="#1678d4", height=0.52)
            ax.text(ratio + 0.025, y, f"{ratio:.2f}×", va="center", fontsize=11, fontweight="bold")
    ax.axvline(1, color="#616b78", linewidth=1.2, linestyle="--", label="Prism = 1.00×")
    ax.set_yticks(range(len(scenarios)), chart_labels)
    ax.invert_yaxis()
    ax.set_xlim(0, max(1.2, max((v for v in ratios if v is not None), default=1) * 1.2))
    ax.set_xlabel("IDG median throughput / Prism median throughput")
    ax.set_title(f"{args.idg_label} vs Prism — same-machine engine trials", loc="left", fontsize=14, pad=20)
    ax.grid(axis="x", color="#e3e7ed", linewidth=0.7)
    ax.set_axisbelow(True)
    for name in ["top", "right", "left"]:
        ax.spines[name].set_visible(False)
    ax.tick_params(axis="y", length=0, pad=10)
    ax.legend(loc="lower right", frameon=False, fontsize=10)
    fig.text(0.02, 0.025, f"{source['repeats']} trials per engine and scenario; bars require every transfer to pass SHA-256 verification.\nMedians, not confidence intervals. One machine; not a universal speed guarantee.", fontsize=9, color="#4b5563")
    fig.subplots_adjust(left=0.30, right=0.97, bottom=0.29 if len(scenarios) <= 3 else 0.18, top=0.78 if len(scenarios) < 3 else 0.84)
    fig.savefig(args.output / "comparison.png", facecolor="white")
    plt.close(fig)


if __name__ == "__main__":
    main()
