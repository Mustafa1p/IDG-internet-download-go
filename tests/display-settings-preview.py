"""Render display-scale and no-scroll Settings checks without touching user settings."""
import json, os, subprocess
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target/display-qa"
OUTPUT.mkdir(parents=True, exist_ok=True)
cases = [(f"settings-min-{tab}", 800, 500, 1, tab) for tab in range(4)]
cases += [(f"settings-{scale}", 960, 640, scale, 0) for scale in [1, 1.25, 1.5, 2]]
cases += [("settings-min-limited", 800, 500, 1, 0)]
cases += [("desktop-1080", 1920, 1080, 1, None), ("desktop-2k", 2560, 1440, 1, None), ("desktop-2k-scaled", 1706, 960, 1.5, None)]
for name, width, height, scale, tab in cases:
    output = OUTPUT / f"{name}.png"
    mode = "--ui-preview" if tab is None else "--settings-preview"
    args = [str(ROOT / "target/debug/IDG.exe"), mode, f"--settings-preview-size={width}x{height}", f"--preview-scale={scale}", f"--settings-preview-output={output}"]
    if tab is not None: args += [f"--preview-settings-tab={tab}"]
    if name == "settings-min-limited": args += ["--settings-preview-limited", "--settings-preview-manual"]
    result = subprocess.run(args, cwd=ROOT, timeout=30, capture_output=True, creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    metadata = json.loads(output.with_suffix(".json").read_text())
    assert abs(metadata["logical_width"] - width) < 1, (name, metadata)
    assert abs(metadata["logical_height"] - height) < 1, (name, metadata)
    assert abs(metadata["pixels_per_point"] - scale) < 0.01, (name, metadata)
    assert metadata["image_width"] == round(width * scale) and metadata["image_height"] == round(height * scale), (name, metadata)
    controls = metadata["controls"]
    for label, (left, top, right, bottom) in controls.items():
        assert left >= 0 and top >= 0 and right <= width + 1 and bottom <= height + 1, (name, label, (left, top, right, bottom))
    assert controls["Search downloads"][2] <= controls["Minimize"][0], (name, controls)
    if tab is not None:
        assert {"Settings section", "Save changes", "Reset to defaults"}.issubset(controls)
        assert controls["Settings section"][3] < controls["Save changes"][1], (name, controls)
        assert controls["Save changes"][3] <= height - (42 if width >= 1100 else 30), (name, controls)
    print(f"{name}: layout bounds, header and Settings controls PASS", flush=True)
