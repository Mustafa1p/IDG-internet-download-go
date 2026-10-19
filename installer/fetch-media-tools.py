"""Fetch pinned upstream media tools, verify SHA-256, and preserve their notices."""
import hashlib
import json
from pathlib import Path
import shutil
import urllib.request
from zipfile import ZipFile

root = Path(__file__).resolve().parents[1]
destination = root / "target/release/tools"
cache = root / "target/media-cache"
destination.mkdir(parents=True, exist_ok=True)
cache.mkdir(parents=True, exist_ok=True)
notices = destination / "licenses"
notices.mkdir(exist_ok=True)
lock = json.loads((root / "installer/media-tools.json").read_text(encoding="utf-8"))


def fetch(url, path):
    request = urllib.request.Request(url, headers={"User-Agent": "IDG-media-tools"})
    with urllib.request.urlopen(request, timeout=120) as response, path.open("wb") as target:
        shutil.copyfileobj(response, target)


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

fetch("https://www.gnu.org/licenses/gpl-3.0.txt", notices / "GPL-3.0.txt")
shutil.copyfile(root / "installer/media-tools.json", notices / "upstream-sources.json")
(notices / "README.txt").write_text(
    "IDG invokes these independent tools as subprocesses. Their licenses and upstream source locations "
    "are included here. yt-dlp's Windows executable and this FFmpeg build contain GPLv3+ components. "
    "Sources: https://github.com/yt-dlp/yt-dlp ; https://github.com/yt-dlp/FFmpeg-Builds ; "
    "https://github.com/FFmpeg/FFmpeg/tree/7045cc9b80 ; https://github.com/denoland/deno\n",
    encoding="utf-8")
for executable in ["yt-dlp.exe", "ffmpeg.exe", "ffprobe.exe", "deno.exe"]:
    if not (destination / executable).is_file():
        raise SystemExit(f"Missing required media tool: {executable}")
