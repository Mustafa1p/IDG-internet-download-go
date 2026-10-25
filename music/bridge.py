# SPDX-License-Identifier: GPL-2.0-only
"""Structured worker for the separately installed GPL OnTheSpot backend.

The IDG desktop speaks JSON over pipes. Service credentials never use that pipe.
See COPYING and upstream.json for licensing and the exact upstream revisions.
"""
import contextlib
import ctypes
from ctypes import wintypes
import json
import importlib
import logging
import os
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile
import threading
import time
import uuid

class BridgeError(RuntimeError):
    """An actionable message that is safe to send to the desktop."""


def safe_message(error):
    return str(error) if isinstance(error, BridgeError) else "Music backend failed. Check the account connection or reinstall the music backend."


PROTOCOL = sys.stdout
SERVICES = ("spotify", "bandcamp", "apple_music", "deezer", "qobuz", "tidal")
TERMINAL = {"Downloaded", "Already Exists", "Failed", "Unavailable", "Cancelled"}


def emit(event, **values):
    PROTOCOL.write(json.dumps({"idg_music": 1, "event": event, **values}) + "\n")
    PROTOCOL.flush()


def protect(data, decrypt=False):
    if os.name != "nt":
        raise BridgeError("Encrypted music accounts require Windows")

    class Blob(ctypes.Structure):
        _fields_ = [("size", wintypes.DWORD), ("data", ctypes.POINTER(ctypes.c_ubyte))]

    buffer = ctypes.create_string_buffer(data)
    source = Blob(len(data), ctypes.cast(buffer, ctypes.POINTER(ctypes.c_ubyte)))
    result = Blob()
    crypt = ctypes.WinDLL("crypt32", use_last_error=True)
    crypt.CryptProtectData.argtypes = [ctypes.POINTER(Blob), wintypes.LPCWSTR, ctypes.POINTER(Blob), ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(Blob)]
    crypt.CryptUnprotectData.argtypes = [ctypes.POINTER(Blob), ctypes.c_void_p, ctypes.POINTER(Blob), ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(Blob)]
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    kernel.LocalFree.restype = ctypes.c_void_p
    if decrypt:
        ok = crypt.CryptUnprotectData(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(result))
    else:
        ok = crypt.CryptProtectData(ctypes.byref(source), "IDG Music", None, None, None, 1, ctypes.byref(result))
    if not ok:
        raise BridgeError("Music accounts could not be unlocked for this Windows user. Reconnect in Music settings.")
    try:
        return ctypes.string_at(result.data, result.size)
    finally:
        kernel.LocalFree(ctypes.cast(result.data, ctypes.c_void_p))


def load_accounts(home):
    path = home / "accounts.bin"
    if not path.exists():
        return []
    return json.loads(protect(path.read_bytes(), decrypt=True))["accounts"]


@contextlib.contextmanager
def account_lock(home):
    import msvcrt
    with (home / "accounts.lock").open("a+b") as lock:
        if lock.tell() == 0:
            lock.write(b"\0")
            lock.flush()
        lock.seek(0)
        msvcrt.locking(lock.fileno(), msvcrt.LK_LOCK, 1)
        try:
            yield
        finally:
            lock.seek(0)
            msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)


def write_accounts(home, accounts):
    data = protect(json.dumps({"accounts": accounts}).encode("utf-8"))
    with tempfile.NamedTemporaryFile(prefix="accounts-", suffix=".pending", dir=home, delete=False) as file:
        file.write(data)
        pending = Path(file.name)
    try:
        os.replace(pending, home / "accounts.bin")
    finally:
        pending.unlink(missing_ok=True)


def save_accounts(home, accounts):
    with account_lock(home):
        write_accounts(home, accounts)


def merge_accounts(initial, current, latest, refresh_service=None):
    """Keep refreshed credentials without overwriting a newer interactive login."""
    services = {a.get("service") for a in initial + current}
    result = list(latest)
    for service in services:
        before = [a for a in initial if a.get("service") == service]
        after = [a for a in current if a.get("service") == service]
        newest = [a for a in latest if a.get("service") == service]
        if refresh_service:
            if service != refresh_service or newest != before:
                continue
            updates = {a.get("uuid"): a for a in after}
            after = [updates.get(a.get("uuid"), a) for a in newest]
        elif before == after:
            continue
        result = [a for a in result if a.get("service") != service] + after
    return result


def account_status(home):
    configured = {a.get("service") for a in load_accounts(home) if a.get("active")}
    return [{"service": service, "configured": service == "bandcamp" or service in configured}
            for service in SERVICES]


def process_alive(pid):
    if os.name != "nt":
        return True
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        return ctypes.get_last_error() != 87  # Access denied is not proof of exit.
    try:
        code = wintypes.DWORD()
        return not kernel.GetExitCodeProcess(handle, ctypes.byref(code)) or code.value == 259
    finally:
        kernel.CloseHandle(handle)


def cleanup_stale_profiles(home):
    base = home.resolve()
    for directory in home.glob("profile-*"):
        if not directory.is_dir() or directory.resolve().parent != base:
            continue
        try:
            pid = json.loads((directory / "owner.json").read_text())["pid"]
            if isinstance(pid, int) and pid > 0 and not process_alive(pid):
                shutil.rmtree(directory)
        except (OSError, ValueError, KeyError):
            continue


@contextlib.contextmanager
def profile(home, writable=False, refresh_service=None):
    # Scope upstream config, cached sessions, and logs to an ephemeral profile.
    with tempfile.TemporaryDirectory(prefix="profile-", dir=home) as tmp:
        root = Path(tmp)
        (root / "owner.json").write_text(json.dumps({"pid": os.getpid()}))
        cache = root / "cache"
        cache.mkdir()
        accounts = load_accounts(home)
        initial = json.loads(json.dumps(accounts))
        if not any(a.get("service") == "bandcamp" for a in accounts):
            accounts.append({"uuid": "public_bandcamp", "service": "bandcamp", "active": True})
        config_path = root / "otsconfig.json"
        config_path.write_text(json.dumps({"version": "v1.1.4", "accounts": accounts,
                                         "audio_download_path": str(root / "audio"), "video_download_path": str(root / "video"),
                                         "close_to_tray": False, "check_for_updates": False}), encoding="utf-8")
        os.environ.update({"ONTHESPOTDIR": str(root), "TEMP": str(cache), "TMP": str(cache),
                           "XDG_CACHE_HOME": str(cache), "LOG_LEVEL": "50",
                           "PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION": "python"})
        logging.disable(logging.CRITICAL)
        completed = False
        try:
            yield root
            completed = True
        finally:
            try:
                if (writable and completed) or refresh_service:
                    module = sys.modules.get("onthespot.otsconfig")
                    if module:
                        module.config.save()
                    current = json.loads(config_path.read_text(encoding="utf-8"))["accounts"]
                    with account_lock(home):
                        merged = merge_accounts(initial, current, load_accounts(home), refresh_service)
                        write_accounts(home, merged)
            finally:
                logging.shutdown()


def finish_state(items, parsing, pending):
    """Only a drained parse/metadata queue and terminal items can finish a job."""
    if not items or parsing or pending or any(i.get("item_status") not in TERMINAL for i in items):
        return None
    failed = [i for i in items if i.get("item_status") not in ("Downloaded", "Already Exists")]
    return "failed" if failed else "complete"


def validate_outputs(items, output):
    paths = []
    for item in items:
        path = Path(item.get("file_path") or "").resolve()
        if not path.is_relative_to(output.resolve()) or not path.is_file() or path.stat().st_size == 0:
            raise BridgeError("The music service did not produce a complete audio file")
        if str(path) not in paths:
            paths.append(str(path))
    return paths


def open_account_window():
    # Do not call onthespot.gui.main(): it ends with os._exit(), which would
    # bypass our encrypted-profile commit and temporary-session cleanup.
    from PyQt6.QtWidgets import QApplication
    from PyQt6.QtGui import QFont, QFontDatabase
    from onthespot.qt.mainui import MainWindow
    from onthespot.qt.minidialog import MiniDialog
    app = QApplication(sys.argv)
    font = Path(os.environ.get("WINDIR", "C:/Windows")) / "Fonts/segoeui.ttf"
    if font.is_file():
        QFontDatabase.addApplicationFont(str(font))
        app.setFont(QFont("Segoe UI", 10))
    dialog = MiniDialog()
    window = MainWindow(dialog, "")
    window.setWindowTitle("IDG Music Accounts · OnTheSpot")
    for index in range(window.tabview.count()):
        page = window.tabview.widget(index)
        label = window.tabview.tabText(index) if hasattr(window.tabview, "tabText") else page.objectName()
        if "setting" in label.lower() or "account" in label.lower():
            window.tabview.setCurrentIndex(index)
            if hasattr(window.tabview, "setTabVisible"):
                for other in range(window.tabview.count()):
                    if other != index:
                        window.tabview.setTabVisible(other, False)
            break
    window.show()
    app.exec()


def download(request, home):
    service = request["service"]
    if service not in SERVICES:
        raise BridgeError("Unsupported original music service")
    output = Path(request["output"]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    with profile(home, refresh_service=service):
        from onthespot.otsconfig import config
        accounts = [a for a in config.get("accounts") if a.get("service") == service and a.get("active")]
        if not accounts:
            raise BridgeError("Connect this service in Settings > Music > Manage accounts before downloading")
        config.set("accounts", accounts)
        overrides = {
            "audio_download_path": str(output), "video_download_path": str(output),
            "_ffmpeg_bin_path": request["ffmpeg"], "active_account_number": 0,
            "track_file_format": request["format"], "podcast_file_format": request["format"],
            "track_path_formatter": "{track_number} - {artist} - {name}",
            "playlist_path_formatter": "{playlist_number} - {artist} - {name}",
            "podcast_path_formatter": "{name}", "use_playlist_path": True,
            "download_delay": 0, "maximum_download_workers": 1, "maximum_queue_workers": 1,
            "rotate_active_account_number": False, "enable_retry_worker": False,
            "mirror_spotify_playback": False, "download_lyrics": False,
            "translate_file_path": False, "create_m3u_file": False,
            "save_album_cover": False, "embed_cover": False, "raw_media_download": False,
            "file_bitrate": f"{request.get('audio_bitrate') or 320}k",
            "use_custom_file_bitrate": request.get("audio_bitrate") is not None,
        }
        for key, value in overrides.items():
            config.set(key, value)
        # Bound requests made by upstream adapters that omit a timeout.
        import requests
        original = requests.sessions.Session.request

        def bounded_request(session, *args, **kwargs):
            kwargs.setdefault("timeout", 30)
            return original(session, *args, **kwargs)

        requests.sessions.Session.request = bounded_request
        from onthespot.accounts import FillAccountPool, get_account_token
        from onthespot.downloader import DownloadWorker
        from onthespot.parse_item import parse_url
        from onthespot import runtimedata as state
        emit("progress", message="Connecting to the original music service", downloaded=0)
        FillAccountPool().run()
        if not any(a.get("service") == service and a.get("status") == "active" for a in state.account_pool):
            raise BridgeError("Service sign-in failed or expired. Reconnect in Settings > Music > Manage accounts.")
        if parse_url(request["url"]) is False:
            raise BridgeError("The music service did not accept this URL")
        # Resolve the complete job before starting downloads. Queue emptiness alone
        # cannot prove completion while upstream parser/metadata threads are busy.
        api = importlib.import_module("onthespot.api." + service)
        token = get_account_token(service)
        with state.parsing_lock:
            roots = list(state.parsing.values())
            state.parsing.clear()
        tracks = []
        for root in roots:
            kind, item_id = root["item_type"], root["item_id"]
            if root["item_service"] != service:
                raise BridgeError("The backend changed music services unexpectedly")
            if kind in ("track", "podcast_episode"):
                tracks.append((kind, item_id))
            elif kind == "album":
                tracks.extend(("track", track) for track in getattr(api, f"{service}_get_album_track_ids")(token, item_id))
            elif kind == "playlist" and service == "spotify":
                for entry in api.spotify_get_playlist_items(token, item_id):
                    track = entry.get("track") or {}
                    if not track.get("id"):
                        raise BridgeError("The playlist includes an unavailable track. Try the available tracks individually.")
                    tracks.append(("podcast_episode" if track.get("type") == "episode" else "track", track["id"]))
            elif kind == "playlist":
                _, _, ids = getattr(api, f"{service}_get_playlist_data")(token, item_id)
                tracks.extend(("track", track) for track in ids)
            elif kind == "podcast":
                tracks.extend(("podcast_episode", track) for track in api.spotify_get_podcast_episode_ids(token, item_id))
            else:
                raise BridgeError("Use a track, album or playlist URL for this service")
        if not tracks or len(tracks) > 10000:
            raise BridgeError("The service returned no tracks or the collection is too large")
        for index, (kind, item_id) in enumerate(tracks):
            metadata = getattr(api, f"{service}_get_{kind}_metadata")(token, item_id)
            if not metadata or not metadata.get("title"):
                raise BridgeError("A track's metadata is unavailable. Check account access or try that track separately.")
            local_id = uuid.uuid4().hex
            try:
                duration_ms = float(metadata.get("length") or 0)
            except (ValueError, TypeError):
                duration_ms = 0
            state.download_queue[local_id] = {
                "local_id": local_id, "available": True, "item_service": service,
                "item_type": kind, "item_id": item_id, "item_status": "Waiting", "file_path": None,
                "item_name": metadata["title"], "item_by": metadata.get("artists", ""),
                "parent_category": "playlist" if len(tracks) > 1 else kind,
                "playlist_name": "IDG", "playlist_by": "", "playlist_number": str(index + 1),
                "expected_duration_ms": duration_ms,
            }
        worker = DownloadWorker()
        worker.thread.daemon = True
        worker.start()
        started = time.monotonic()
        settled = None
        while time.monotonic() - started < 1800:
            with state.download_queue_lock:
                items = [dict(item) for item in state.download_queue.values()]
            with state.parsing_lock:
                parsing = bool(state.parsing)
            with state.pending_lock:
                pending = bool(state.pending)
            status = finish_state(items, parsing, pending)
            if status:
                settled = settled or time.monotonic()
                if time.monotonic() - settled > 2:
                    if status == "failed":
                        raise BridgeError("One or more tracks failed or are unavailable. The job was not marked complete; reconnect the account or check service availability.")
                    paths = validate_outputs(items, output)
                    files = [{"path": path, "duration_ms": next((i.get("expected_duration_ms", 0) for i in items if str(Path(i.get("file_path") or "").resolve()) == path), 0)} for path in paths]
                    emit("complete", files=files, service=service)
                    return
            else:
                settled = None
            if not items and time.monotonic() - started > 90:
                raise BridgeError("No tracks were returned. Check the URL, account access and region.")
            downloaded = sum(p.stat().st_size for p in output.rglob("*") if p.is_file())
            emit("progress", message=f"{service}: {sum(i.get('item_status') == 'Downloaded' for i in items)}/{len(items)} tracks complete", downloaded=downloaded)
            time.sleep(0.5)
        raise BridgeError("Music download timed out. Check the service connection and retry.")


def main(request):
    home = Path(request["home"]).resolve()
    home.mkdir(parents=True, exist_ok=True)
    cleanup_stale_profiles(home)
    if request.get("ffmpeg"):
        os.environ["FFMPEG_PATH"] = request["ffmpeg"]
    action = request["action"]
    if action == "status":
        emit("status", accounts=account_status(home))
    elif action == "download":
        download(request, home)
    elif action == "pair_spotify":
        with profile(home, writable=True):
            from onthespot.otsconfig import config
            config.set("accounts", [a for a in config.get("accounts") if a.get("service") != "spotify"])
            emit("progress", message="In Spotify Desktop, open Connect to a device and select OnTheSpot. Waiting up to 3 minutes.")
            from onthespot.api.spotify import spotify_new_session
            if not spotify_new_session():
                raise BridgeError("Spotify pairing did not finish")
        emit("status", accounts=account_status(home))
    elif action == "accounts":
        with profile(home, writable=True):
            emit("progress", message="Music account manager is open. Close it to return to IDG.")
            open_account_window()
        emit("status", accounts=account_status(home))
    else:
        raise BridgeError("Unknown music action")


if __name__ == "__main__":
    request = json.loads(sys.stdin.buffer.readline(65537))
    # Upstream prints/logs may contain account information. Only our protocol is emitted.
    exit_code = 0
    with open(os.devnull, "w") as sink, contextlib.redirect_stdout(sink), contextlib.redirect_stderr(sink):
        try:
            main(request)
        except Exception as error:
            # Deliberately emit only our actionable errors, never raw upstream exceptions.
            message = safe_message(error)
            emit("error", message=message)
            exit_code = 1
    os._exit(exit_code)
