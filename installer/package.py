"""Package a source build without installing any packaging tools."""
from pathlib import Path
import json
import tomllib
import sys
from zipfile import ZipFile, ZIP_DEFLATED

root = Path(__file__).resolve().parents[1]
version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
tag = sys.argv[1] if len(sys.argv) > 1 else f"v{version}"
if tag != f"v{version}":
    raise SystemExit(f"Release tag {tag!r} does not match Cargo version v{version}")
if json.loads((root / "extension/manifest.json").read_text(encoding="utf-8"))["version"] != version:
    raise SystemExit("Extension and application versions do not match")
for binary in ("IDG.exe", "IDG-native.exe"):
    if not (root / "target/release" / binary).is_file():
        raise SystemExit("Run cargo build --release --bins first.")
tools = root / "target/release/tools"
for name in ("yt-dlp.exe", "ffmpeg.exe", "ffprobe.exe", "deno.exe", "licenses/upstream-sources.json"):
    if not (tools / name).is_file():
        raise SystemExit("Run python installer/fetch-media-tools.py before packaging; media tools must be included.")
(root / "dist").mkdir(exist_ok=True)
extension_files = sorted(path for path in (root / "extension").rglob("*") if path.is_file())
extension_zip = root / f"dist/IDG-extension-{tag}.zip"
with ZipFile(extension_zip, "w", ZIP_DEFLATED) as archive:
    for path in extension_files:
        archive.write(path, path.relative_to(root / "extension"))
portable_zip = root / f"dist/IDG-portable-{tag}.zip"
with ZipFile(portable_zip, "w", ZIP_DEFLATED) as archive:
    for binary in ("IDG.exe", "IDG-native.exe"):
        archive.write(root / "target/release" / binary, binary)
    for path in extension_files:
        archive.write(path, path.relative_to(root))
    for path in sorted(tools.rglob("*")):
        if path.is_file():
            archive.write(path, Path("tools") / path.relative_to(tools))
    for name in ("installer/register-native-host.ps1", "README.md"):
        archive.write(root / name, name)
    # Keep README screenshots, route guides and QA report links usable offline.
    for path in sorted((root / "docs").rglob("*")):
        if path.is_file():
            archive.write(path, path.relative_to(root))
print(f"Created {portable_zip.name} and {extension_zip.name}")
