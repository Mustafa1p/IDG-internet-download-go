"""Real child-process resource accounting, including after exit and on failure."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("process_metrics", ROOT / "scripts/process_metrics.py")
metrics = importlib.util.module_from_spec(spec)
spec.loader.exec_module(metrics)


@unittest.skipUnless(os.name == "nt", "Windows process counters")
class MetricsTests(unittest.TestCase):
    def test_lifetime_peaks_and_cpu_remain_available_after_process_exit(self):
        code = """import time
data = bytearray(32 * 1024 * 1024)
deadline = time.process_time() + 0.15
while time.process_time() < deadline:
    data[0] = (data[0] + 1) % 256
"""
        with subprocess.Popen([sys.executable, "-c", code], creationflags=subprocess.CREATE_NO_WINDOW) as child:
            self.assertEqual(child.wait(timeout=10), 0)
            usage = metrics.snapshot(child)
            self.assertEqual(usage["errors"], [])
            self.assertGreaterEqual(usage["peak_working_set_bytes"], 32 * 1024 * 1024)
            self.assertGreaterEqual(usage["peak_commit_bytes"], 32 * 1024 * 1024)
            self.assertGreaterEqual(usage["cpu_total_seconds"], 0.10)
            self.assertAlmostEqual(usage["cpu_total_seconds"], usage["cpu_user_seconds"] + usage["cpu_kernel_seconds"])

    def test_invalid_handle_reports_unavailable_counters_without_inventing_zero(self):
        class Closed:
            _handle = 0
        usage = metrics.snapshot(Closed())
        self.assertEqual(len(usage["errors"]), 2)
        self.assertNotIn("cpu_total_seconds", usage)
        self.assertNotIn("peak_working_set_bytes", usage)


if __name__ == "__main__":
    unittest.main()
