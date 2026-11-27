"""Fetch pinned upstream media tools, verify SHA-256, and preserve their notices."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request
from zipfile import ZipFile

root = Path(__file__).resolve().parents[1]
arch = sys.argv[1] if len(sys.argv) > 1 else "x64"
if arch not in ("x64", "x86"): raise SystemExit("Architecture must be x64 or x86")
build = "target/i686-pc-windows-msvc/release" if arch == "x86" else "target/release"
destination = root / build / "tools"
cache = root / "target/media-cache"
destination.mkdir(parents=True, exist_ok=True)
cache.mkdir(parents=True, exist_ok=True)
notices = destination / "licenses"
notices.mkdir(exist_ok=True)
torrent_runtime = destination / "torrent"
torrent_runtime.mkdir(exist_ok=True)
lock_path = root / ("installer/media-tools-x86.json" if arch == "x86" else "installer/media-tools.json")
lock = json.loads(lock_path.read_text(encoding="utf-8"))


def fetch(url, path):
    request = urllib.request.Request(url, headers={"User-Agent": "IDG-media-tools"})
    with urllib.request.urlopen(request, timeout=120) as response, path.open("wb") as target:
        shutil.copyfileobj(response, target)


def stage_cpp_runtime():
    # Use Microsoft's redistributable files from the installed Visual Studio toolchain.
    vswhere = Path(os.environ["ProgramFiles(x86)"]) / "Microsoft Visual Studio/Installer/vswhere.exe"
    if not vswhere.is_file():
        raise SystemExit("Install the Visual Studio C++ build tools and VC++ Redistributable component.")
    installs = subprocess.check_output(
        [str(vswhere), "-products", "*", "-format", "json", "-utf8"], encoding="utf-8-sig"
    )
    candidates = [
        runtime
        for installation in json.loads(installs)
        for runtime in (Path(installation["installationPath"]) / "VC/Redist/MSVC")
        .glob(f"*/{arch}/Microsoft.VC*.CRT")
        if (runtime / "msvcp140.dll").is_file()
    ]
    if not candidates:
        raise SystemExit(f"Install the Visual Studio C++ Redistributable component for {arch}.")
    runtime = max(candidates, key=lambda path: tuple(map(int, path.parent.parent.name.split("."))))
    files = ["msvcp140.dll", "vcruntime140.dll"]
    if arch == "x64":
        files.append("vcruntime140_1.dll")
    hashes = {}
    for name in files:
        shutil.copyfile(runtime / name, torrent_runtime / name)
        with (torrent_runtime / name).open("rb") as source:
            hashes[name] = hashlib.file_digest(source, "sha256").hexdigest()
    (notices / "vc-runtime.json").write_text(json.dumps({
        "architecture": arch,
        "toolchain_redistributable_directory": runtime.parent.parent.name,
        "source": "https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files",
        "sha256": hashes,
    }, indent=2) + "\n", encoding="utf-8")


for tool in lock["tools"]:
    archive = cache / (tool["sha256"] + (".exe" if tool["format"] == "exe" else ".zip"))
    if not archive.is_file():
        print(f"Downloading {tool['name']} {tool['tag']}", flush=True)
        fetch(tool["url"], archive)
    with archive.open("rb") as source:
        actual = hashlib.file_digest(source, "sha256").hexdigest()
    if actual != tool["sha256"]:
        raise SystemExit(f"Checksum mismatch for {tool['name']}; delete {archive} and retry")
    if tool["format"] == "exe":
        shutil.copyfile(archive, destination / (tool["name"] + ".exe"))
    else:
        with ZipFile(archive) as package:
            for member in package.infolist():
                path = Path(member.filename)
                if member.is_dir():
                    continue
                if tool["format"] == "torrent-python":
                    if len(path.parts) == 1:
                        target = torrent_runtime / path.name
                        with package.open(member) as source, target.open("wb") as output:
                            shutil.copyfileobj(source, output)
                    continue
                if tool["format"] == "torrent-wheel":
                    # Keep the extension in its wheel package; importlib loads its .pyd.
                    if len(path.parts) == 2 and path.parts[0] == "libtorrent" and path.suffix == ".pyd":
                        target = torrent_runtime / path
                        target.parent.mkdir(exist_ok=True)
                        with package.open(member) as source, target.open("wb") as output:
                            shutil.copyfileobj(source, output)
                    continue
                is_binary = (path.suffix.lower() in {".exe", ".dll"} and path.name != "ffplay.exe"
                             and (tool["format"] == "zip" or "bin" in path.parts))
                is_notice = "license" in path.name.lower() or "copying" in path.name.lower()
                if is_binary or is_notice:
                    output = destination / path.name if is_binary else notices / (tool["name"] + "-" + path.name)
                    with package.open(member) as source, output.open("wb") as target:
                        shutil.copyfileobj(source, target)
    for index, url in enumerate(tool["licenses"]):
        fetch(url, notices / f"{tool['name']}-{index}-{url.rsplit('/', 1)[-1]}.txt")
    print(f"Verified {tool['name']} {tool['tag']}", flush=True)

stage_cpp_runtime()
shutil.copyfile(root / "torrent/worker.py", torrent_runtime / "worker.py")
if not (torrent_runtime / "python.exe").is_file() or not any((torrent_runtime / "libtorrent").glob("*.pyd")):
    raise SystemExit("Missing bundled BitTorrent runtime or module")

# The complete GPLv3 text was already fetched from FFmpeg's pinned source revision.
# Reuse it rather than making release packaging depend on another host's uptime.
shutil.copyfile(notices / "ffmpeg-0-COPYING.GPLv3.txt", notices / "GPL-3.0.txt")
shutil.copyfile(lock_path, notices / "upstream-sources.json")
(notices / "README.txt").write_text(
    "IDG invokes these independent tools as subprocesses. Their licenses and upstream source locations "
    "are included here. yt-dlp's Windows executable and this FFmpeg build contain GPLv3+ components. "
    "Sources: https://github.com/yt-dlp/yt-dlp ; https://github.com/yt-dlp/FFmpeg-Builds ; "
    "https://github.com/FFmpeg/FFmpeg/tree/7045cc9b80 ; https://github.com/denoland/deno ; https://github.com/nodejs/node\n",
    encoding="utf-8")
for executable in ["yt-dlp.exe", "ffmpeg.exe", "ffprobe.exe", "node.exe" if arch == "x86" else "deno.exe"]:
    if not (destination / executable).is_file():
        raise SystemExit(f"Missing required media tool: {executable}")
