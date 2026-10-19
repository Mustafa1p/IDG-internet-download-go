"""Windows smoke test of the built bridge launching the real desktop app.

Run after `cargo build --release --bins` with IDG closed.
Only processes created by this test are terminated. No network file is downloaded.
"""
import json
from pathlib import Path
import struct
import subprocess
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BASE = "http://127.0.0.1:17843"
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
try:
    opener.open(f"{BASE}/health", timeout=1)
except (urllib.error.URLError, OSError):
    pass
else:
    raise SystemExit("Close IDG before running the cold-start smoke test.")

host = subprocess.Popen(
    [str(ROOT / "target/release/IDG-native.exe")],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    creationflags=subprocess.CREATE_NO_WINDOW,
)


def request(message):
    data = json.dumps(message).encode()
    host.stdin.write(struct.pack("=I", len(data)) + data)
    host.stdin.flush()
    length = host.stdout.read(4)
    assert len(length) == 4, "Native host exited before responding"
    return json.loads(host.stdout.read(struct.unpack("=I", length)[0]))


try:
    ping = request({"type": "ping"})
    assert ping["installed"] and not ping["serverOnline"], ping
    print("Native ping detects the bridge while IDG is closed: PASS", flush=True)
    reply = request({"type": "download", "download": {
        "url": "https://example.com/IDG-smoke.zip",
        "filename": "IDG-smoke.zip", "show_dialog": True,
    }})
    assert reply["ok"], reply
    with opener.open(f"{BASE}/downloads", timeout=5) as response:
        queue = json.load(response)
    assert queue[0]["filename"] == "IDG-smoke.zip" and queue[0]["status"] == "Queued", queue
    assert request({"type": "ping"})["serverOnline"]
    print("Native bridge launches release IDG and queues confirmation: PASS", flush=True)
finally:
    subprocess.run(["taskkill", "/PID", str(host.pid), "/T", "/F"],
                   capture_output=True, creationflags=subprocess.CREATE_NO_WINDOW)
