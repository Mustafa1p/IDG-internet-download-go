"""Protect retained latency evidence from invalid settings and overwrites."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/benchmark_probe_latency.py"


class EvidenceGuards(unittest.TestCase):
    def run_fixture(self, directory, *arguments):
        output = directory / "result.json"
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--baseline", str(directory / "absent.exe"),
             "--candidate", str(directory / "absent.exe"), "--output", str(output),
             *arguments], capture_output=True, text=True, timeout=10,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Use a new output file", result.stderr)
        self.assertFalse((directory / "result-files").exists())
        return output

    def test_invalid_settings_fail_before_creating_evidence_or_reading_binaries(self):
        for option, value in [
            ("--latency-ms", "nan"), ("--latency-ms", "inf"),
            ("--latency-ms", "-1"), ("--per-connection-mib", "nan"),
            ("--per-connection-mib", "inf"), ("--per-connection-mib", "-1"),
            ("--first-range-mib", "nan"), ("--first-range-mib", "-1"),
            ("--slow-range-start-kib", "-1"), ("--slow-range-start-kib", "4096"),
            ("--size-kib", "0"), ("--connections", "-1"),
            ("--connections", "33"), ("--repeats", "0"),
        ]:
            with self.subTest(option=option, value=value), tempfile.TemporaryDirectory() as temporary:
                output = self.run_fixture(Path(temporary), option, value)
                self.assertFalse(output.exists())

    def test_existing_evidence_remains_byte_identical(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            output = directory / "result.json"
            original = b"previous evidence must survive\x00\xff"
            output.write_bytes(original)
            self.run_fixture(directory)
            self.assertEqual(output.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
