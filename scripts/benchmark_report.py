"""Create the repository's benchmark tables/chart from retained raw trials."""
import argparse
import json
from pathlib import Path
import shutil
import statistics

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ["curl", "aria2c", "Prism", "IDG"]
LABELS = {
    "A": "1 GiB, 4 MiB/s per connection",
    "B": "2 GiB, unlimited localhost",
    "C": "256 MiB, four connections at 2 MiB/s",
    "D": "256 MiB, 2% random drops per chunk",
    "E": "76.2 MiB public Rust archive over HTTPS",
}


def summarize(runs):
    output = {}
    for scenario in LABELS:
        output[scenario] = {}
        for tool in TOOLS:
            rows = [r for r in runs if r["scenario"] == scenario and r["tool"] == tool]
            assert len(rows) == (1 if tool == "curl" else 3), (scenario, tool, len(rows))
            for r in rows:
                if r["verified"]:
                    assert r["expected_sha256"] == r["actual_sha256"]
                    assert r["actual_size"] == r["size"]
                    assert r["exit_code"] == 0 and not r["sync_error"]
                    assert abs(r["mib_per_second"] - r["size"] / 1048576 / r["durable_seconds"]) < 1e-8
            successful = [r for r in rows if r["verified"]]
            rates = [r["mib_per_second"] for r in successful]
            output[scenario][tool] = {
                "attempts": len(rows), "verified": len(successful),
                "median_mib_s": statistics.median(rates) if rates else None,
                "min_mib_s": min(rates) if rates else None,
                "max_mib_s": max(rates) if rates else None,
                "median_seconds": statistics.median(r["durable_seconds"] for r in successful) if rates else None,
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
    args = p.parse_args()
    source = json.loads(args.results.read_text(encoding="utf-8"))
    assert source["size_factor"] == 1 and "finished_utc" in source
    summary = summarize(source["runs"])
    args.output.mkdir(parents=True, exist_ok=True)
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

    lines = ["| Scenario | curl (n=1) | aria2c (n=3) | Prism 1.1.7 (n=3) | IDG 1.6.1 (n=3) |",
             "| --- | ---: | ---: | ---: | ---: |"]
    for scenario, label in LABELS.items():
        lines.append("| " + label + " | " + " | ".join(cell(summary[scenario][tool]) for tool in TOOLS) + " |")
    table = "\n".join(lines)
    (args.output / "table.md").write_text(table + "\n", encoding="utf-8")
    detail = ["| Scenario | Tool | Verified / attempted | Median seconds | Median MiB/s | Observed min–max MiB/s |",
              "| --- | --- | ---: | ---: | ---: | ---: |"]
    for scenario in LABELS:
        for tool in TOOLS:
            s = summary[scenario][tool]
            time = f"{s['median_seconds']:.3f}" if s["verified"] else "—"
            median = f"{s['median_mib_s']:.2f}" if s["verified"] else "—"
            spread = f"{s['min_mib_s']:.2f}–{s['max_mib_s']:.2f}" if s["verified"] else "—"
            detail.append(f"| {scenario} | {tool} | {s['verified']} / {s['attempts']} | {time} | {median} | {spread} |")
    (args.output / "detail.md").write_text("\n".join(detail) + "\n", encoding="utf-8")

    # A static figure is appropriate for the requested GitHub Markdown surface.
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 11})
    fig, ax = plt.subplots(figsize=(11.2, 5.7), dpi=160)
    ratios = []
    for scenario in LABELS:
        i, p = summary[scenario]["IDG"], summary[scenario]["Prism"]
        ratios.append(i["median_mib_s"] / p["median_mib_s"] if i["verified"] == 3 and p["verified"] == 3 else None)
    labels = ["A  Per-connection throttle", "B  Unlimited localhost", "C  Four-connection server", "D  Random connection drops", "E  Public HTTPS archive"]
    for y, ratio in enumerate(ratios):
        if ratio is None:
            ax.text(0.04, y, "Not comparable: at least one failed trial", va="center", fontsize=10)
        else:
            ax.barh(y, ratio, color="#1678d4", height=0.52)
            ax.text(ratio + 0.025, y, f"{ratio:.2f}×", va="center", fontsize=11, fontweight="bold")
    ax.axvline(1, color="#616b78", linewidth=1.2, linestyle="--", label="Prism = 1.00×")
    ax.set_yticks(range(5), labels)
    ax.invert_yaxis()
    ax.set_xlim(0, max(1.2, max((v for v in ratios if v is not None), default=1) * 1.2))
    ax.set_xlabel("IDG median throughput / Prism median throughput")
    ax.set_title("IDG 1.6.1 vs Prism 1.1.7 — same-machine engine trials", loc="left", fontsize=14, pad=20)
    ax.grid(axis="x", color="#e3e7ed", linewidth=0.7)
    ax.set_axisbelow(True)
    for name in ["top", "right", "left"]:
        ax.spines[name].set_visible(False)
    ax.tick_params(axis="y", length=0, pad=10)
    ax.legend(loc="lower right", frameon=False, fontsize=10)
    fig.text(0.02, 0.025, "Three trials per engine and scenario; all results SHA-256 checked. Bars show medians, not confidence intervals.\nHigher than 1.00× means higher measured throughput. One machine; not a universal speed guarantee.", fontsize=9, color="#4b5563")
    fig.subplots_adjust(left=0.30, right=0.97, bottom=0.18, top=0.84)
    fig.savefig(args.output / "comparison.png", facecolor="white")
    plt.close(fig)
    print(table)


if __name__ == "__main__":
    main()
