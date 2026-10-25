"""Reject stale installer versions before spending time building release assets."""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import re
import json
import tomllib


class PackageVersionTests(unittest.TestCase):
    def test_repository_release_versions_match(self):
        root = Path(__file__).resolve().parents[1]
        version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
        extension = json.loads((root / "extension/manifest.json").read_text())["version"]
        installer = re.search(r'^#define AppVersion\s+"([^\"]+)"', (root / "installer/setup.iss").read_text(), re.MULTILINE)
        self.assertIsNotNone(installer)
        self.assertEqual(installer[1], version)
        self.assertEqual(extension, version)
        package = next(p for p in tomllib.loads((root / "Cargo.lock").read_text())["package"] if p["name"] == "idg")
        self.assertEqual(package["version"], version)

    def test_installer_version_must_match_before_binary_checks(self):
        source = Path(__file__).resolve().parents[1]
        for installer_version, expected in [
            ("1.0.0", "Run cargo build --release --bins first."),
            ("0.9.0", "Installer and application versions do not match"),
        ]:
            with self.subTest(installer_version=installer_version), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                (root / "installer").mkdir()
                (root / "extension").mkdir()
                shutil.copyfile(source / "installer/package.py", root / "installer/package.py")
                (root / "Cargo.toml").write_text('[package]\nversion = "1.0.0"\n', encoding="utf-8")
                (root / "extension/manifest.json").write_text('{"version":"1.0.0"}', encoding="utf-8")
                (root / "installer/setup.iss").write_text(f'#define AppVersion "{installer_version}"\n', encoding="utf-8")
                result = subprocess.run([sys.executable, str(root / "installer/package.py")], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(expected, result.stderr)


if __name__ == "__main__":
    unittest.main()
