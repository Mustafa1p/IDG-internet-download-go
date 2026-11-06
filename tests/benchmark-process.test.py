"""Real subprocess checks for timed completion, fsync, failure and timeout."""
from pathlib import Path
import os
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from benchmark_process import measure_client


class ProcessTests(unittest.TestCase):
    def run_client(self, root, code, timeout=10):
        return measure_client([sys.executable, "-c", code, str(root / "download.bin")],
                              root / "download.bin", root / "tool.log", timeout)

    def test_exact_output_success_has_lifetime_resources_and_durability_time(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            value = self.run_client(root, "import pathlib,sys; pathlib.Path(sys.argv[1]).write_bytes(b'bytes'*8192); print('done')")
            self.assertEqual(value["exit_code"], 0)
            self.assertIsNone(value["sync_error"])
            self.assertEqual((root / "download.bin").read_bytes(), b"bytes" * 8192)
            self.assertIn("done", (root / "tool.log").read_text())
            self.assertGreaterEqual(value["durable_seconds"], value["process_seconds"])
            if os.name == "nt":
                self.assertEqual(value["resources"]["errors"], [])
                self.assertGreater(value["resources"]["peak_working_set_bytes"], 0)

    def test_nonzero_exit_and_timeout_preserve_partial_file_and_diagnostics(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for tail, expected in [("raise SystemExit(7)", 7), ("time.sleep(10)", "timeout")]:
                value = self.run_client(root, "import pathlib,sys,time; pathlib.Path(sys.argv[1]).write_bytes(b'partial'); print('failure',flush=True); " + tail, timeout=1.5)
                self.assertEqual(value["exit_code"], expected)
                self.assertEqual((root / "download.bin").read_bytes(), b"partial")
                self.assertIn("failure", (root / "tool.log").read_text())
                self.assertIsNone(value["sync_error"])
                if os.name == "nt":
                    self.assertEqual(value["resources"]["errors"], [])

    def test_fsync_failure_is_not_converted_to_success(self):
        with tempfile.TemporaryDirectory() as temp, patch("benchmark_process.os.fsync", side_effect=OSError("disk failure")):
            value = self.run_client(Path(temp), "import pathlib,sys; pathlib.Path(sys.argv[1]).write_bytes(b'partial')")
            self.assertEqual(value["exit_code"], 0)
            self.assertIn("disk failure", value["sync_error"])

    def test_resource_collection_is_outside_transfer_timing(self):
        import time
        with tempfile.TemporaryDirectory() as temp:
            def slow_snapshot(child):
                time.sleep(0.12)
                return {"test": True}
            with patch("benchmark_process.snapshot", slow_snapshot):
                before = time.perf_counter()
                value = self.run_client(Path(temp), "pass")
                elapsed = time.perf_counter() - before
            self.assertGreaterEqual(elapsed - value["durable_seconds"], 0.11)
            self.assertEqual(value["resources"], {"test": True})


if __name__ == "__main__":
    unittest.main()
