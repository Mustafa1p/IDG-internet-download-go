"""Click real egui format/quality controls at representative window sizes/scales."""
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
report = root / "target/fluent-qa/interactive.json"
results = []
for width, height, scale in [(800, 500, 1), (800, 500, 1.25), (800, 500, 1.5), (800, 500, 2), (1280, 800, 1)]:
    report.unlink(missing_ok=True)
    result = subprocess.run([str(root / "target/debug/IDG.exe"), "--ui-preview", "--ui-preview-hold",
        "--preview-media", "--preview-self-test", f"--settings-preview-size={width}x{height}",
        f"--preview-scale={scale}"], cwd=root, capture_output=True, timeout=40,
        creationflags=subprocess.CREATE_NO_WINDOW)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    data = json.loads(report.read_text())
    assert data["passed"] and len(data["checks"]) == 7, data
    assert all(check["passed"] for check in data["checks"]), data
    results.append({"size": [width, height], "scale": scale, "checks": data["checks"]})
    print(f"PASS media dialog {width}x{height} at {scale}: MP3, MP4, 480p, footer remains clickable", flush=True)
(root / "target/fluent-qa/media-dialog.json").write_text(json.dumps(results, indent=2))
