# SPDX-License-Identifier: GPL-2.0-only
"""Install IDG's optional, version-pinned original-service music worker."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import urllib.request
import venv
import zipfile


def fetch_source(pin, destination):
    url = f"https://codeload.github.com/{pin['repository']}/zip/{pin['revision']}"
    with urllib.request.urlopen(url, timeout=60) as response:
        data = response.read()
    if hashlib.sha256(data).hexdigest() != pin["archive_sha256"]:
        raise RuntimeError("Music source checksum did not match the pinned version")
    archive = destination / "source.zip"
    archive.write_bytes(data)
    with zipfile.ZipFile(archive) as source:
        for entry in source.infolist():
            path = (destination / entry.filename).resolve()
            if not path.is_relative_to(destination.resolve()):
                raise RuntimeError("Invalid source archive path")
        source.extractall(destination)
    return next(path for path in destination.iterdir() if path.is_dir())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--home", required=True, type=Path)
    parser.add_argument("--ffmpeg", required=True, type=Path)
    args = parser.parse_args()
    if sys.version_info[:2] != (3, 12) or sys.maxsize <= 2**32:
        raise RuntimeError("Music setup requires Python 3.12 (64-bit). Install it from python.org, then retry.")
    home = args.home.resolve()
    home.mkdir(parents=True, exist_ok=True)
    pins = json.loads(Path(__file__).with_name("upstream.json").read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="sources-", dir=home) as tmp:
        source_root = Path(tmp)
        ots_root = source_root / "onthespot"
        lib_root = source_root / "librespot"
        ots_root.mkdir()
        lib_root.mkdir()
        ots = fetch_source(pins["onthespot"], ots_root)
        lib = fetch_source(pins["librespot"], lib_root)
        runtime = Path(tempfile.mkdtemp(prefix="runtime-", dir=home))
        venv.EnvBuilder(with_pip=True).create(runtime)
        python = runtime / "Scripts/python.exe"
        # The upstream direct Git dependency is supplied from our verified archive.
        dependencies = [line for line in Path(__file__).with_name("requirements-lock.txt").read_text().splitlines()
                        if line.strip() and not line.startswith("#")]
        log = home / "setup.log"
        with log.open("w", encoding="utf-8") as output:
            for packages in [[str(lib)], dependencies, ["--no-deps", str(ots)]]:
                result = subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check", *packages],
                                        stdout=output, stderr=subprocess.STDOUT)
                if result.returncode:
                    raise RuntimeError(f"Music dependency installation failed. See {log}")
            check_profile = source_root / "import-profile"
            check_profile.mkdir()
            result = subprocess.run([str(python), "-c", "import onthespot.cli; import librespot"],
                                    env={**os.environ, "ONTHESPOTDIR": str(check_profile), "TEMP": str(check_profile), "FFMPEG_PATH": str(args.ffmpeg.resolve())},
                                    stdout=output, stderr=subprocess.STDOUT)
            if result.returncode:
                raise RuntimeError(f"Music worker import check failed. See {log}")
        notices = runtime / "upstream-source"
        notices.mkdir()
        # Preserve the exact corresponding source, including notices, beside the worker.
        import shutil
        shutil.copytree(ots, notices / "onthespot")
        shutil.copytree(lib, notices / "librespot")
        frozen = subprocess.check_output([str(python), "-m", "pip", "freeze"], text=True)
        (runtime / "installed-requirements.txt").write_text(frozen, encoding="utf-8")
        marker = {"protocol": 1, "python": str(python), "pins": pins}
        pending = home / "runtime.pending.json"
        pending.write_text(json.dumps(marker), encoding="utf-8")
        os.replace(pending, home / "runtime.json")
    print("Music backend installed. Open Music settings to connect your accounts.")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
