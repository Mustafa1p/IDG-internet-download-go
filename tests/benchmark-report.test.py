"""Reject misleading performance reports and exercise the report CLI."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/benchmark_report.py"
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location("benchmark_report", SCRIPT)
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def trial(tool="IDG", repeat=1, *, verified=True):
    return dict(scenario="E", tool=tool, repeat=repeat, verified=verified,
                expected_sha256="a" * 64, actual_sha256="a" * 64,
                actual_size=1048576, size=1048576, exit_code=0,
                sync_error=None, durable_seconds=1.0, mib_per_second=1.0,
                candidate_policy="production", log=f"logs/{tool}-{repeat}/tool.log")


class ReportTests(unittest.TestCase):
    def summary(self, rows, repeats=1):
        return report.summarize(rows, tools=["IDG"], scenarios=["E"],
                                repeats=repeats, curl_repeats=repeats)

    def test_failed_transfers_remain_visible_and_disqualify_comparison(self):
        failed = trial(repeat=2, verified=False)
        failed.update(exit_code="timeout", actual_sha256="b" * 64,
                      mib_per_second=None, durable_seconds=90.0)
        value = self.summary([trial(), failed], 2)["E"]["IDG"]
        self.assertEqual((value["verified"], value["attempts"]), (1, 2))
        self.assertFalse(value["all_verified"])
        self.assertIn("1/2 verified", report.cell(value))

    def test_duplicate_repeat_cannot_replace_a_missing_trial(self):
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.summary([trial(), trial()], 2)

    def test_corrupt_bytes_or_timings_cannot_claim_verified(self):
        for change in [dict(actual_sha256="b" * 64), dict(actual_size=1),
                       dict(exit_code=1), dict(sync_error="disk full"),
                       dict(durable_seconds=0), dict(durable_seconds=float("nan")),
                       dict(mib_per_second=1000), dict(verified="true")]:
            with self.subTest(change=change), self.assertRaises(ValueError):
                row = trial()
                row.update(change)
                self.summary([row])

    def test_different_policies_or_missing_tools_are_not_combined(self):
        row = trial(repeat=2)
        row["candidate_policy"] = "http2"
        with self.assertRaisesRegex(ValueError, "policies"):
            self.summary([trial(), row], 2)
        with self.assertRaises(ValueError):
            report.summarize([trial()], tools=["IDG", "IDG-baseline"],
                             scenarios=["E"], repeats=1, curl_repeats=1)

    def test_cli_retains_seven_round_baseline_and_rejects_overwrite(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            rows = [trial(tool, n) for tool in ["IDG", "IDG-baseline"] for n in range(1, 8)]
            for row in rows:
                row["resources"] = dict(errors=[], cpu_total_seconds=0.5,
                                        peak_working_set_bytes=1048576, peak_commit_bytes=2097152,
                                        cpu_cycles=2_000_000_000)
                log = root / row["log"]
                log.parent.mkdir(parents=True)
                log.write_text("fixture diagnostics")
            source = dict(size_factor=1, finished_utc="2026-11-06T00:00:00Z", repeats=7,
                          binary_sha256={"IDG": "b" * 64, "IDG-baseline": "c" * 64},
                          idg_commit="test-candidate", runs=rows)
            raw = root / "raw.json"
            raw.write_text(json.dumps(source))
            command = [sys.executable, str(SCRIPT), str(raw), "--output", str(root / "report"),
                       "--scenarios", "E", "--no-chart", "--idg-label", "IDG candidate"]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("IDG-baseline (n=7)", result.stdout)
            self.assertIn("IDG candidate (n=7)", result.stdout)
            self.assertEqual(json.loads((root / "report/results.json").read_text()), source)
            self.assertEqual((root / "report/logs/IDG-7/tool.log").read_text(), "fixture diagnostics")
            self.assertIn("Includes measured failed attempts", (root / "report/resources.md").read_text())
            self.assertIn("7 / 7 | 0.500 | 1.000 | 2.000", (root / "report/resources.md").read_text())
            self.assertIn("7 / 7 | 2000.000", (root / "report/resources.md").read_text())
            self.assertIn("no conversion to seconds", (root / "report/resources.md").read_text())
            self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)

    def test_different_source_files_are_not_compared_as_one_workload(self):
        row = trial(repeat=2)
        row.update(expected_sha256="b" * 64, actual_sha256="b" * 64)
        with self.assertRaisesRegex(ValueError, "mixed source"):
            self.summary([trial(), row], 2)

    def test_different_connection_settings_cannot_disappear_into_a_median(self):
        rows = [trial(), trial(repeat=2)]
        rows[0]["requested_connections"] = 1
        rows[1]["requested_connections"] = 16
        with self.assertRaisesRegex(ValueError, "connection settings"):
            self.summary(rows, 2)

    def test_resource_summary_includes_failed_attempts_and_excludes_unavailable(self):
        rows = [trial(repeat=i) for i in range(1, 4)]
        rows[0]["resources"] = dict(errors=[], cpu_total_seconds=1.0,
                                    peak_working_set_bytes=1048576, peak_commit_bytes=2097152, cpu_cycles=100)
        rows[1].update(verified=False, exit_code="timeout", mib_per_second=None)
        rows[1]["resources"] = dict(errors=[], cpu_total_seconds=5.0,
                                    peak_working_set_bytes=8388608, peak_commit_bytes=16777216, cpu_cycles=300)
        rows[2]["resources"] = dict(errors=["unavailable"])
        value = self.summary(rows, 3)["E"]["IDG"]
        self.assertEqual(value["resource_samples"], 2)
        self.assertEqual(value["median_client_cpu_seconds"], 3.0)
        self.assertEqual(value["max_client_peak_working_set_mib"], 8.0)
        self.assertEqual(value["max_client_peak_commit_mib"], 16.0)
        self.assertEqual(value["cycle_samples"], 2)
        self.assertEqual(value["median_client_cpu_cycles"], 200)
        rows[0]["resources"]["cpu_total_seconds"] = -1.0
        with self.assertRaisesRegex(ValueError, "resource counter"):
            self.summary(rows, 3)
        rows[0]["resources"]["cpu_total_seconds"] = 1.0
        for invalid in [True, -1, float("nan"), 1 << 64]:
            rows[0]["resources"]["cpu_cycles"] = invalid
            with self.assertRaisesRegex(ValueError, "cycle counter"):
                self.summary(rows, 3)

    def test_incomplete_or_missing_scenario_creates_no_report(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = dict(size_factor=1, repeats=1, binary_sha256={"IDG": "b" * 64}, runs=[trial()])
            for finished in [None, "2026-11-06T00:00:00Z"]:
                candidate = copy.deepcopy(source)
                candidate["finished_utc"] = finished
                raw = root / "raw.json"
                raw.write_text(json.dumps(candidate))
                result = subprocess.run([sys.executable, "-O", str(SCRIPT), str(raw), "--output",
                                         str(root / "report"), "--scenarios", "A", "E", "--no-chart"],
                                        capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((root / "report").exists())


if __name__ == "__main__":
    unittest.main()
