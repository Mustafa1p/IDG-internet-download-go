"""Click development-build egui controls at representative sizes/scales.

IDG_UI_BINARY may select another development build, including x86. Release
builds deliberately omit the preview harness; this is not a package smoke test.
"""
import json
import os
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
binary = Path(os.environ.get("IDG_UI_BINARY", str(root / "target/debug/IDG.exe"))).resolve()
report = root / "target/fluent-qa/interactive.json"
results = []
for width, height, scale in [(800, 500, 1), (800, 500, 1.25), (800, 500, 1.5), (800, 500, 2), (1280, 800, 1)]:
    report.unlink(missing_ok=True)
    result = subprocess.run([str(binary), "--ui-preview", "--ui-preview-hold",
        "--preview-media", "--preview-self-test", f"--settings-preview-size={width}x{height}",
        f"--preview-scale={scale}"], cwd=root, capture_output=True, timeout=40,
        creationflags=subprocess.CREATE_NO_WINDOW)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    assert report.is_file(), "Preview report missing; IDG_UI_BINARY must be a development build with the preview harness enabled"
    data = json.loads(report.read_text())
    assert data["passed"] and len(data["checks"]) == 7, data
    assert all(check["passed"] for check in data["checks"]), data
    results.append({"size": [width, height], "scale": scale, "checks": data["checks"]})
    print(f"PASS media dialog {width}x{height} at {scale}: MP3, MP4, 480p, footer remains clickable", flush=True)
(root / "target/fluent-qa/media-dialog.json").write_text(json.dumps(results, indent=2))
