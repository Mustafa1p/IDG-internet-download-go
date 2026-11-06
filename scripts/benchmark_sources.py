"""Pinned public HTTPS workloads; download these artifacts without executing them."""
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import PurePosixPath
import urllib.request

HTTPS_SOURCES = {
    "E": dict(label="76.2 MiB public Rust archive over HTTPS", size=79909984,
              url="https://static.rust-lang.org/dist/rustc-1.90.0-x86_64-pc-windows-msvc.tar.xz",
              sha256="a18b3912905f154c34a7572954dda9e9849ac6411a1e02fa390bf742f918fbcb",
              checksum_url="https://static.rust-lang.org/dist/rustc-1.90.0-x86_64-pc-windows-msvc.tar.xz.sha256",
              checksum_format="sha256"),
    "F": dict(label="34.7 MiB official Node ZIP over HTTPS", size=36356854,
              url="https://nodejs.org/dist/v24.11.0/node-v24.11.0-win-x64.zip",
              sha256="1054540bce22b54ec7e50ebc078ec5d090700a77657607a58f6a64df21f49fdd",
              checksum_url="https://nodejs.org/dist/v24.11.0/SHASUMS256.txt",
              checksum_format="sha256"),
    "G": dict(label="26.9 MiB official Python installer over HTTPS", size=28160096,
              url="https://www.python.org/ftp/python/3.13.0/python-3.13.0-amd64.exe",
              sha256="78156ad0cf0ec4123bfb5333b40f078596ebf15f2d062a10144863680afbdefc",
              checksum_url="https://www.python.org/ftp/python/3.13.0/python-3.13.0-amd64.exe.sigstore",
              checksum_format="sigstore-digest"),
}


def check_reference(source, payload):
    if source["checksum_format"] == "sigstore-digest":
        digest = json.loads(payload)["messageSignature"]["messageDigest"]
        if digest["algorithm"] != "SHA2_256":
            raise ValueError("Expected the publisher's SHA-256 digest")
        actual = base64.b64decode(digest["digest"], validate=True).hex()
    else:
        filename = PurePosixPath(source["url"]).name
        matching = [parts[0] for line in payload.decode("utf-8").splitlines()
                    if len(parts := line.split()) == 2 and parts[1].lstrip("*") == filename]
        if len(matching) != 1:
            raise ValueError("Missing or ambiguous publisher checksum filename")
        actual = matching[0]
    if actual != source["sha256"]:
        raise ValueError(f"Publisher checksum changed for {source['url']}")
    return actual


def verify_sources(keys):
    """Check pinned hashes against official metadata over HTTPS, not signatures."""
    def verify(key):
        source = HTTPS_SOURCES[key]
        with urllib.request.urlopen(source["checksum_url"], timeout=30) as response:
            payload = response.read()
        return key, dict(checksum_url=source["checksum_url"], sha256=check_reference(source, payload),
                         metadata_sha256=hashlib.sha256(payload).hexdigest(),
                         verification="Pinned SHA-256 matched official publisher metadata over HTTPS")
    keys = [key for key in keys if key in HTTPS_SOURCES]
    if not keys:
        return {}
    with ThreadPoolExecutor(max_workers=min(3, len(keys))) as pool:
        return dict(pool.map(verify, keys))
