"""Windows desktop lifecycle test; requires an interactive desktop and IDG closed.

Exercises only windows belonging to the process this test creates. Menu command
IDs 1000/1001 and tray callback 6002 match the tray-icon/muda versions in Cargo.toml.
"""
import ctypes
from ctypes import wintypes
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BASE = "http://127.0.0.1:17843"
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
user32 = ctypes.WinDLL("user32", use_last_error=True)
CALLBACK = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
user32.EnumWindows.argtypes = [CALLBACK, wintypes.LPARAM]
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]


def api(path, payload=None):
    request = urllib.request.Request(BASE + path)
    if payload is not None:
        request = urllib.request.Request(BASE + path, data=json.dumps(payload).encode(),
                                         headers={"Content-Type": "application/json"})
    with opener.open(request, timeout=1) as response:
        return json.load(response) if response.status != 204 else None


def wait_for(check, description, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            result = check()
            if result:
                return result
        except (urllib.error.URLError, OSError):
            pass
        time.sleep(0.05)
    raise AssertionError(description)


def windows(pid):
    result = {}

    @CALLBACK
    def visit(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid:
            name = ctypes.create_unicode_buffer(256)
            user32.GetClassNameW(hwnd, name, 256)
            title = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd, title, 256)
            if name.value == "tray_icon_app":
                result["tray"] = hwnd
            elif title.value.startswith("IDG"):
                result["main"] = hwnd
        return True

    user32.EnumWindows(visit, 0)
    return result


DATA = bytes(range(256)) * 128


class Source(BaseHTTPRequestHandler):
    def do_HEAD(self):
        self.send_response(200)
        self.send_header("Content-Length", str(len(DATA)))
        self.end_headers()

    def do_GET(self):
        self.do_HEAD()
        for offset in range(0, len(DATA), 1024):
            self.wfile.write(DATA[offset:offset + 1024])
            self.wfile.flush()
            time.sleep(0.03)

    def log_message(self, *_):
        pass


try:
    api("/health")
except (urllib.error.URLError, OSError):
    pass
else:
    raise SystemExit("Close IDG before running the tray smoke test.")

directory = Path(tempfile.mkdtemp(prefix="idg-tray-smoke-")).resolve()
source = ThreadingHTTPServer(("127.0.0.1", 0), Source)
threading.Thread(target=source.serve_forever, daemon=True).start()
app = subprocess.Popen([str(ROOT / "target/release/IDG.exe"), "--background"],
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                       creationflags=subprocess.CREATE_NO_WINDOW)
try:
    wait_for(lambda: api("/health"), "API did not start")
    handles = wait_for(lambda: (value if len(value := windows(app.pid)) == 2 else None),
                       "Main window or tray icon was not created")
    main, tray = handles["main"], handles["tray"]
    wait_for(lambda: not user32.IsWindowVisible(main), "Background launch left the main window visible")
    print("Background startup creates the tray icon and hides the window: PASS", flush=True)

    user32.PostMessageW(tray, 0x0111, 1000, 0)  # Open IDG menu command
    wait_for(lambda: user32.IsWindowVisible(main), "Tray Open IDG did not restore the hidden window")
    user32.PostMessageW(main, 0x0010, 0, 0)  # Close main window
    wait_for(lambda: not user32.IsWindowVisible(main), "Close did not hide to tray")
    assert app.poll() is None
    print("Open IDG and close-to-tray keep the process alive: PASS", flush=True)

    api("/downloads", {"url": f"http://127.0.0.1:{source.server_port}/file",
                       "filename": "background.bin", "save_dir": str(directory)})
    wait_for(lambda: api("/downloads")[0]["status"] == "Completed", "Background download did not finish")
    assert (directory / "background.bin").read_bytes() == DATA
    assert not user32.IsWindowVisible(main)
    print("Download finishes with exact bytes while the window stays hidden: PASS", flush=True)

    duplicate = subprocess.run([str(ROOT / "target/release/IDG.exe")], timeout=10,
                               creationflags=subprocess.CREATE_NO_WINDOW, capture_output=True)
    assert duplicate.returncode == 0
    wait_for(lambda: user32.IsWindowVisible(main), "Second launch did not restore the original window")
    print("A second launch reopens the existing instance: PASS", flush=True)

    user32.PostMessageW(main, 0x0010, 0, 0)
    wait_for(lambda: not user32.IsWindowVisible(main), "Window did not hide again")
    user32.PostMessageW(tray, 6002, 0, 0x0202)  # Tray left-button release
    wait_for(lambda: user32.IsWindowVisible(main), "Tray click did not reopen IDG")
    print("Left-clicking the tray icon reopens IDG: PASS", flush=True)

    user32.PostMessageW(main, 0x0010, 0, 0)
    wait_for(lambda: not user32.IsWindowVisible(main), "Window did not hide again")
    api("/downloads", {"url": "https://example.com/confirmation.zip", "show_dialog": True})
    wait_for(lambda: user32.IsWindowVisible(main), "Browser confirmation did not restore hidden IDG")
    print("Browser downloads restore the hidden confirmation window: PASS", flush=True)

    user32.PostMessageW(main, 0x0010, 0, 0)
    wait_for(lambda: not user32.IsWindowVisible(main), "Window did not hide before quit")
    user32.PostMessageW(tray, 0x0111, 1001, 0)  # Quit IDG menu command
    assert app.wait(timeout=10) == 0
    print("Quit IDG exits cleanly from the hidden tray state: PASS", flush=True)
finally:
    if app.poll() is None:
        app.terminate()
        app.wait(timeout=5)
    source.shutdown()
    assert directory.is_relative_to(Path(tempfile.gettempdir()).resolve())
    assert directory.name.startswith("idg-tray-smoke-")
    shutil.rmtree(directory)
