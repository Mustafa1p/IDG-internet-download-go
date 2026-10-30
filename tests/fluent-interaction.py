"""Exercise the actual debug app's controls without OS-wide input injection."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "target/fluent-qa/interactive.json"
REPORT.unlink(missing_ok=True)
result = subprocess.run(
    [str(Path(os.environ.get("IDG_UI_BINARY", str(ROOT / "target/debug/IDG.exe")))), "--ui-preview", "--ui-preview-hold",
     "--preview-populated", "--preview-self-test", "--settings-preview-size=1280x800",
     "--preview-scale=1"],
    cwd=ROOT, capture_output=True, timeout=45,
    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
)
assert result.returncode == 0, result.stderr.decode(errors="replace")
report = json.loads(REPORT.read_text(encoding="utf-8"))
assert report["passed"] and not report.get("in_progress", False), report.get("error")
assert len(report["checks"]) == 48
assert all(check["passed"] for check in report["checks"])
print("Actual app interactions: 48/48 PASS; Settings tabs, speed-limit draft, native minimize/restore and maximize/restore verified.")
