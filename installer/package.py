"""Package a source build without installing any packaging tools."""
from pathlib import Path
import json
import tomllib
import sys
import struct
import subprocess
import re
from zipfile import ZipFile, ZIP_DEFLATED

root = Path(__file__).resolve().parents[1]
version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
tag = sys.argv[1] if len(sys.argv) > 1 and sys.argv[1] not in ("x64", "x86") else f"v{version}"
if tag != f"v{version}":
    raise SystemExit(f"Release tag {tag!r} does not match Cargo version v{version}")
if json.loads((root / "extension/manifest.json").read_text(encoding="utf-8"))["version"] != version:
    raise SystemExit("Extension and application versions do not match")
installer_version = re.search(r'^#define AppVersion\s+"([^"]+)"', (root / "installer/setup.iss").read_text(encoding="utf-8"), re.MULTILINE)
if not installer_version or installer_version[1] != version:
    raise SystemExit("Installer and application versions do not match")
arch = sys.argv[2] if len(sys.argv) > 2 else (sys.argv[1] if len(sys.argv) > 1 and sys.argv[1] in ("x64", "x86") else "x64")
if arch not in ("x64", "x86"): raise SystemExit("Architecture must be x64 or x86")
build = root / ("target/i686-pc-windows-msvc/release" if arch == "x86" else "target/release")
suffix = "-x86" if arch == "x86" else ""
for binary in ("IDG.exe", "IDG-native.exe"):
    if not (build / binary).is_file():
        raise SystemExit("Run cargo build --release --bins first.")
for name in ("bridge.py", "setup.py", "upstream.json", "requirements-lock.txt", "COPYING"):
    if not (root / "music" / name).is_file():
        raise SystemExit(f"Missing music backend source or license: {name}")
tools = build / "tools"
for name in ("yt-dlp.exe", "ffmpeg.exe", "ffprobe.exe", "node.exe" if arch == "x86" else "deno.exe", "licenses/upstream-sources.json"):
    if not (tools / name).is_file():
        raise SystemExit("Run python installer/fetch-media-tools.py before packaging; media tools must be included.")
# Reject mislabeled packages, including incompatible helper executables and DLLs.
for path in [build / "IDG.exe", build / "IDG-native.exe", *tools.rglob("*.exe"), *tools.rglob("*.dll")]:
    with path.open("rb") as source:
        if source.read(2) != b"MZ": raise SystemExit(f"Invalid Windows binary: {path}")
        source.seek(0x3c)
        offset = struct.unpack("<I", source.read(4))[0]
        source.seek(offset)
        if source.read(4) != b"PE\0\0": raise SystemExit(f"Invalid PE header: {path}")
        machine = struct.unpack("<H", source.read(2))[0]
    if machine != (0x14c if arch == "x86" else 0x8664):
        raise SystemExit(f"Wrong architecture in {arch} package: {path}")
# A same-version update must identify exactly the source build being published.
try:
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, stderr=subprocess.DEVNULL).strip()
except (FileNotFoundError, subprocess.CalledProcessError):
    revision = None
if revision and revision not in (build / "IDG.exe").read_bytes():
    raise SystemExit("IDG.exe does not contain the current source build identity. Rebuild after committing before packaging.")
(root / "dist").mkdir(exist_ok=True)
extension_files = sorted(path for path in (root / "extension").rglob("*") if path.is_file())
extension_zip = root / f"dist/IDG-extension-{tag}.zip"
with ZipFile(extension_zip, "w", ZIP_DEFLATED) as archive:
    for path in extension_files:
        archive.write(path, path.relative_to(root / "extension"))
portable_zip = root / f"dist/IDG-portable-{tag}{suffix}.zip"
with ZipFile(portable_zip, "w", ZIP_DEFLATED) as archive:
    for binary in ("IDG.exe", "IDG-native.exe"):
        archive.write(build / binary, binary)
    for path in extension_files:
        archive.write(path, path.relative_to(root))
    for path in sorted(tools.rglob("*")):
        if path.is_file():
            archive.write(path, Path("tools") / path.relative_to(tools))
    for path in sorted((root / "music").iterdir()):
        if path.is_file() and (path.suffix in (".py", ".txt", ".json") or path.name == "COPYING"):
            archive.write(path, Path("music") / path.name)
    for name in ("installer/register-native-host.ps1", "README.md"):
        archive.write(root / name, name)
    # Keep README screenshots, route guides and QA report links usable offline.
    for path in sorted((root / "docs").rglob("*")):
        if path.is_file():
            archive.write(path, path.relative_to(root))
print(f"Created {portable_zip.name} and {extension_zip.name}")
