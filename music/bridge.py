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


def pair_spotify(root, notify, cancelled=None, timeout=180):
    """Advertise IDG directly through the pinned Connect library."""
    from librespot.zeroconf import ZeroconfServer
    from onthespot.otsconfig import config
    cancelled = cancelled or threading.Event()
    credentials = root / "spotify-session.json"
    fields = ZeroconfServer._ZeroconfServer__default_get_info_fields
    fields.update({"clientID": "65b708073fc0480ea92a077233ca87bd",
                   "brandDisplayName": "IDG", "modelDisplayName": "Internet Download Go"})
    builder = ZeroconfServer.Builder()
    builder.device_name = "IDG"
    builder.conf.stored_credentials_file = str(credentials)
    server = builder.create()
    try:
        notify("In Spotify Desktop, open Connect to a device and select IDG. Waiting up to 3 minutes.")
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if cancelled.is_set():
                raise BridgeError("Spotify pairing cancelled. Your saved account was kept.")
            if server.has_valid_session() and credentials.is_file():
                session = json.loads(credentials.read_text(encoding="utf-8"))
                if not all(session.get(key) for key in ("username", "credentials", "type")):
                    raise BridgeError("Spotify returned an incomplete session. Retry Connect Spotify.")
                account = {"uuid": uuid.uuid4().hex, "service": "spotify", "active": True,
                           "login": {key: session[key] for key in ("username", "credentials", "type")}}
                config.set("accounts", [a for a in config.get("accounts") if a.get("service") != "spotify"] + [account])
                config.save()
                return
            cancelled.wait(0.25)
        raise BridgeError("Spotify pairing timed out. Retry Connect Spotify and select IDG in Spotify Desktop.")
    finally:
        try:
            server.close_session()
        finally:
            server.close()
            credentials.unlink(missing_ok=True)


def configure_account(service, values, notify, cancelled):
    import requests
    original = requests.sessions.Session.request

    def bounded_request(session, *args, **kwargs):
        if cancelled.is_set():
            raise BridgeError("Connection cancelled. Your saved account was kept.")
        kwargs.setdefault("timeout", 20)
        return original(session, *args, **kwargs)

    requests.sessions.Session.request = bounded_request
    try:
        _configure_account(service, values, notify, cancelled)
    finally:
        requests.sessions.Session.request = original


def _configure_account(service, values, notify, cancelled):
    """Use service libraries without importing their application or account UI."""
    from onthespot.otsconfig import config
    initial = json.loads(json.dumps(config.get("accounts")))
    try:
        config.set("accounts", [a for a in initial if a.get("service") != service])
        if service == "apple_music":
            if not values[0].strip():
                raise BridgeError("Enter your Apple Music media user token.")
            from onthespot.api.apple_music import apple_music_add_account
            apple_music_add_account(values[0].strip())
        elif service == "deezer":
            if not values[0].strip():
                raise BridgeError("Enter your Deezer ARL session cookie.")
            from onthespot.api.deezer import deezer_add_account
            deezer_add_account(values[0].strip())
        elif service == "qobuz":
            if not values[0].strip() or not values[1]:
                raise BridgeError("Enter your Qobuz email and password.")
            from onthespot.api.qobuz import qobuz_add_account
            qobuz_add_account(values[0].strip(), values[1])
        elif service == "tidal":
            from onthespot.api import tidal
            from urllib.parse import urlparse
            import requests
            # Use bounded, cancellable OAuth polling rather than the library's
            # unbounded GUI loop. Authorization stays on Tidal's own website.
            response = requests.post(f"{tidal.AUTH_URL}/device_authorization",
                                     data={"client_id": tidal.CLIENT_ID, "scope": "r_usr+w_usr+w_sub"}, timeout=20)
            response.raise_for_status()
            device = response.json()
            url = device["verificationUriComplete"]
            if not url.startswith("https://"):
                url = "https://" + url
            parsed = urlparse(url)
            if parsed.scheme != "https" or parsed.hostname not in ("tidal.com", "login.tidal.com", "link.tidal.com") or parsed.username or parsed.password:
                raise BridgeError("Tidal returned an unexpected sign-in address.")
            notify(url)
            deadline = time.monotonic() + min(float(device.get("expiresIn", 180)), 180)
            interval = max(1, min(float(device.get("interval", 3)), 10))
            while time.monotonic() < deadline and not cancelled.wait(interval):
                response = requests.post(f"{tidal.AUTH_URL}/token", auth=tidal.AUTH,
                    data={"client_id": tidal.CLIENT_ID, "device_code": device["deviceCode"],
                          "grant_type": "urn:ietf:params:oauth:grant-type:device_code", "scope": "r_usr+w_usr+w_sub"}, timeout=20)
                data = response.json()
                if response.status_code == 200 and data.get("access_token"):
                    login = {"username": data["user"]["username"], "country_code": data["user"]["countryCode"],
                             "access_token": data["access_token"], "refresh_token": data["refresh_token"],
                             "token_expiry": time.time() + float(data["expires_in"])}
                    config.set("accounts", config.get("accounts") + [{"uuid": uuid.uuid4().hex,
                               "service": "tidal", "active": True, "login": login}])
                    break
                error = data.get("error")
                if error == "slow_down":
                    interval = min(interval + 5, 30)
                elif error != "authorization_pending" and data.get("sub_status") != 1002:
                    raise BridgeError("Tidal sign-in failed or expired. Try connecting again.")
            else:
                raise BridgeError("Tidal sign-in cancelled or timed out. Your saved account was kept.")
        else:
            raise BridgeError("Use Connect Spotify in IDG Music settings. Bandcamp needs no account.")
        if cancelled.is_set():
            raise BridgeError("Connection cancelled. Your saved account was kept.")
        if not any(a.get("service") == service for a in config.get("accounts")):
            raise BridgeError("Sign-in failed. Check the account details and try again.")
        config.save()
    except Exception:
        config.set("accounts", initial)
        config.save()
        raise


def create_account_window():
    """IDG-owned account UI; upstream GUI modules are never loaded."""
    from PyQt6.QtCore import QThread, pyqtSignal, QUrl
    from PyQt6.QtGui import QDesktopServices
    from PyQt6.QtWidgets import (QDialog, QVBoxLayout, QFormLayout, QLabel,
                                QComboBox, QLineEdit, QPushButton, QHBoxLayout)
    from onthespot.otsconfig import config

    class AccountTask(QThread):
        result = pyqtSignal(str)
        authorization = pyqtSignal(str)

        def __init__(self, service, values, parent):
            super().__init__(parent)
            self.service, self.values = service, values
            self.cancelled = threading.Event()

        def run(self):
            try:
                configure_account(self.service, self.values, self.authorization.emit, self.cancelled)
                self.result.emit("")
            except Exception as error:
                self.result.emit(safe_message(error))
            finally:
                self.values = None

    class AccountWindow(QDialog):
        def __init__(self):
            super().__init__()
            self.task = None
            self.setWindowTitle("IDG Music Accounts")
            self.setMinimumSize(540, 420)
            self.setStyleSheet("QDialog {background:#f5f7fb;color:#172339;} QLabel {color:#172339;} QLineEdit,QComboBox {padding:8px;background:white;color:#172339;border:1px solid #8b98ad;border-radius:4px;} QPushButton {padding:9px 16px;background:white;color:#172339;border:1px solid #8b98ad;border-radius:4px;} QPushButton:disabled {background:#e7ecf3;color:#64748b;border-color:#c0cad8;} QPushButton:focus {border:2px solid #225ed8;}")
            layout = QVBoxLayout(self)
            layout.setContentsMargins(24, 22, 24, 22)
            heading = QLabel("IDG Music Accounts")
            heading.setStyleSheet("font-size:21px;font-weight:600;")
            layout.addWidget(heading)
            intro = QLabel("Connect your music services here. Account data is encrypted for your Windows user when you close this window.")
            intro.setWordWrap(True)
            layout.addWidget(intro)
            self.service = QComboBox()
            for label, key in [("Spotify", "spotify"), ("Bandcamp", "bandcamp"), ("Apple Music", "apple_music"), ("Deezer", "deezer"), ("Qobuz", "qobuz"), ("Tidal", "tidal")]:
                self.service.addItem(label, key)
            form = QFormLayout()
            form.addRow("Service", self.service)
            self.first_label, self.second_label = QLabel(), QLabel("Password")
            self.first, self.second = QLineEdit(), QLineEdit()
            self.first.setEchoMode(QLineEdit.EchoMode.Password)
            self.second.setEchoMode(QLineEdit.EchoMode.Password)
            form.addRow(self.first_label, self.first)
            form.addRow(self.second_label, self.second)
            layout.addLayout(form)
            self.help = QLabel()
            self.help.setWordWrap(True)
            layout.addWidget(self.help)
            self.status = QLabel()
            self.status.setWordWrap(True)
            layout.addWidget(self.status)
            layout.addStretch()
            buttons = QHBoxLayout()
            self.connect_button = QPushButton("Connect")
            self.connect_button.setStyleSheet("QPushButton:enabled {background:#225ed8;color:white;border:1px solid #225ed8;}")
            self.remove_button = QPushButton("Disconnect service")
            self.close_button = QPushButton("Save and close")
            for button in (self.connect_button, self.remove_button, self.close_button):
                buttons.addWidget(button)
            layout.addLayout(buttons)
            self.service.currentIndexChanged.connect(self.update_service)
            self.connect_button.clicked.connect(self.connect_service)
            self.remove_button.clicked.connect(self.remove_service)
            self.close_button.clicked.connect(self.accept)
            self.update_service()

        def update_service(self):
            key = self.service.currentData()
            self.first.clear()
            self.second.clear()
            self.first.setVisible(key in ("apple_music", "deezer", "qobuz"))
            self.first_label.setVisible(key in ("apple_music", "deezer", "qobuz"))
            self.second.setVisible(key == "qobuz")
            self.second_label.setVisible(key == "qobuz")
            self.first.setEchoMode(QLineEdit.EchoMode.Normal if key == "qobuz" else QLineEdit.EchoMode.Password)
            self.first_label.setText({"apple_music": "Media user token", "deezer": "ARL session cookie", "qobuz": "Email"}.get(key, ""))
            self.help.setText({"spotify": "Use Connect Spotify in IDG Settings > Music, then select IDG in Spotify Desktop's device picker.", "bandcamp": "Public Bandcamp downloads need no account.", "apple_music": "Enter the media user token from your own Apple Music web session. Sign-in is checked when downloading.", "deezer": "Enter the ARL cookie from your own Deezer web session. Sign-in is checked when downloading.", "qobuz": "Enter your Qobuz account details. A network sign-in checks them before adding the account.", "tidal": "Connect opens Tidal's sign-in website. Return here after authorizing; IDG waits for up to 3 minutes."}[key])
            configured = any(a.get("service") == key and a.get("active") for a in config.get("accounts"))
            self.status.setText("Public access" if key == "bandcamp" else "Configured; sign-in is checked when downloading" if configured else "Not connected")
            self.connect_button.setEnabled(key not in ("spotify", "bandcamp"))
            self.remove_button.setEnabled(configured and key != "bandcamp")

        def remove_service(self):
            key = self.service.currentData()
            config.set("accounts", [a for a in config.get("accounts") if a.get("service") != key])
            config.save()
            self.update_service()

        def connect_service(self):
            if self.task and self.task.isRunning():
                return
            self.task = AccountTask(self.service.currentData(), [self.first.text(), self.second.text()], self)
            self.first.clear()
            self.second.clear()
            for widget in (self.service, self.first, self.second, self.connect_button, self.remove_button):
                widget.setEnabled(False)
            self.close_button.setText("Cancel connection")
            self.status.setText("Connecting. Please wait...")
            self.task.authorization.connect(self.authorize)
            self.task.result.connect(self.show_result)
            self.task.finished.connect(self.finish_task)
            self.task.start()

        def authorize(self, url):
            self.status.setText("Complete sign-in in your browser, then return here. Waiting for Tidal authorization...")
            QDesktopServices.openUrl(QUrl(url))

        def show_result(self, error):
            self.connection_error = error

        def finish_task(self):
            for widget in (self.service, self.first, self.second):
                widget.setEnabled(True)
            self.close_button.setText("Save and close")
            self.update_service()
            error = getattr(self, "connection_error", "")
            self.status.setText(error or "Account configured. Save and close to keep it in IDG. Sign-in is checked when downloading.")

        def reject(self):
            self.accept()

        def accept(self):
            if self.task and self.task.isRunning():
                self.task.cancelled.set()
                self.status.setText("Cancelling connection; waiting for the current request to finish...")
                return
            super().accept()

        def closeEvent(self, event):
            if self.task and self.task.isRunning():
                self.accept()
                event.ignore()
            else:
                event.accept()

    return AccountWindow()


def open_account_window():
    from PyQt6.QtWidgets import QApplication
    from PyQt6.QtGui import QFont
    app = QApplication(sys.argv)
    app.setApplicationName("IDG")
    app.setFont(QFont("Segoe UI", 10))
    window = create_account_window()
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
        with profile(home, writable=True) as root:
            pair_spotify(root, lambda message: emit("progress", message=message))
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
