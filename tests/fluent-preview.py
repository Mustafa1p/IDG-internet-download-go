"""Render the actual Rust desktop app in isolated development-preview mode.

These are egui rendering-scale checks, not changes to Windows display settings.
Fixture download rows never start network work or write user downloads.
"""
import json
import os
from pathlib import Path
import struct
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target/fluent-qa"
OUTPUT.mkdir(parents=True, exist_ok=True)
BINARY = Path(os.environ.get("IDG_UI_BINARY", str(ROOT / "target/debug/IDG.exe")))
CASES = [
    (f"empty-{width}x{height}", width, height, 1.0, [])
    for width, height in [(800, 500), (1280, 720), (1366, 768), (1600, 900), (1920, 1080)]
] + [
    (f"scale-{scale}", 800, 500, scale, []) for scale in [1.25, 1.5, 2.0]
] + [
    ("reference", 960, 640, 1.0, []),
    ("compact-list", 960, 640, 1.0, ["--preview-populated", "--preview-selected"]),
    ("compact-update", 960, 640, 1.0, ["--update-preview"]),
    ("list", 1280, 800, 1.0, ["--preview-populated"]),
    ("expanded", 1440, 900, 1.0, ["--preview-populated", "--preview-expanded"]),
    ("long-name-selected", 1280, 800, 1.0, ["--preview-populated", "--preview-selected", "--preview-long-name"]),
    ("selected", 1280, 800, 1.0, ["--preview-populated", "--preview-selected"]),
    ("grid", 1280, 800, 1.0, ["--preview-populated", "--preview-grid"]),
    ("grid-minimum", 800, 500, 1.0, ["--preview-populated", "--preview-grid"]),
    ("grid-compact", 960, 640, 1.0, ["--preview-populated", "--preview-grid"]),
    ("completed-selected", 960, 640, 1.0, ["--preview-populated", "--preview-selected-completed"]),
    ("completed-selected-tall", 1280, 800, 1.0, ["--preview-populated", "--preview-selected-completed"]),
    ("list-minimum", 800, 500, 1.0, ["--preview-populated"]),
    ("settings", 1440, 1000, 1.0, ["--preview-settings"]),
    ("settings-minimum", 800, 500, 1.0, ["--preview-settings"]),
    ("add", 1280, 800, 1.0, ["--preview-add"]),
    ("add-minimum", 800, 500, 1.0, ["--preview-add"]),
    ("update", 1280, 800, 1.0, ["--update-preview"]),
    ("remove", 1280, 800, 1.0, ["--preview-populated", "--preview-selected", "--preview-remove"]),
]
results = []
for name, width, height, scale, extra in CASES:
    image = OUTPUT / f"{name}.png"
    mode = "--settings-preview" if "--preview-settings" in extra else "--ui-preview"
    extra = [argument for argument in extra if argument != "--preview-settings"]
    command = [str(BINARY), mode, f"--settings-preview-size={width}x{height}",
               f"--preview-scale={scale}", f"--settings-preview-output={image}", *extra]
    completed = subprocess.run(command, cwd=ROOT, timeout=30, capture_output=True,
                               creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    assert completed.returncode == 0, completed.stderr.decode(errors="replace")
    with image.open("rb") as stream:
        header = stream.read(24)
    actual = struct.unpack(">II", header[16:24])
    assert actual == (round(width * scale), round(height * scale)), (name, actual)
    metadata = json.loads(image.with_suffix(".json").read_text())
    assert abs(metadata["logical_width"] - width) <= 1.0, (name, metadata)
    assert abs(metadata["logical_height"] - height) <= 1.0, (name, metadata)
    assert abs(metadata["pixels_per_point"] - scale) < 0.01, (name, metadata)
    assert metadata["version"] == tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    for label in ["Settings", "Download folder", "Other"]:
        left, top, right, bottom = metadata["controls"][label]
        assert 0 <= left < right <= width and 0 <= top < bottom <= height, (name, label, metadata["controls"][label])
    results.append({"name": name, "logical_size": [width, height], "scale": scale, "raster": actual})
    print(f"{name}: actual app rendered {actual[0]}x{actual[1]}, scale {scale}: PASS", flush=True)
(OUTPUT / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
