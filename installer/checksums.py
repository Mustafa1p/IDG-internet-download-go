"""Fail a release with missing assets; emit SHA-256 checksums for its downloads."""
import hashlib
from pathlib import Path
import sys
import tomllib

root = Path(__file__).resolve().parents[1]
version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
tag = sys.argv[1] if len(sys.argv) > 1 else f"v{version}"
if tag != f"v{version}":
    raise SystemExit("Release tag does not match the application version")
names = [f"IDG-Setup-{tag}.exe", f"IDG-portable-{tag}.zip", f"IDG-extension-{tag}.zip", f"IDG-Setup-{tag}-x86.exe", f"IDG-portable-{tag}-x86.zip"]
lines = []
for name in names:
    path = root / "dist" / name
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"Missing or empty release asset: {name}")
    with path.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    lines.append(f"{digest}  {name}")
(root / "dist/SHA256SUMS.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
print("Verified all release assets and wrote SHA256SUMS.txt")
