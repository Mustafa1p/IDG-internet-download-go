"""Music job completion and Windows credential persistence regressions."""
import importlib.util
import os
from pathlib import Path
import sys
import json
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
import threading

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("music_bridge", Path(__file__).resolve().parents[1] / "music/bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class MusicBridgeTests(unittest.TestCase):
    def test_busy_parser_and_metadata_work_cannot_report_success(self):
        complete = [{"item_status": "Downloaded"}]
        self.assertIsNone(bridge.finish_state([], False, False))
        self.assertIsNone(bridge.finish_state(complete, True, False))
        self.assertIsNone(bridge.finish_state(complete, False, True))
        self.assertIsNone(bridge.finish_state(complete + [{"item_status": "Converting"}], False, False))
        self.assertEqual(bridge.finish_state(complete, False, False), "complete")
        self.assertEqual(bridge.finish_state(complete + [{"item_status": "Failed"}], False, False), "failed")

    def test_output_manifest_rejects_missing_empty_and_outside_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = root / "output"
            output.mkdir()
            outside = root / "outside.mp3"
            outside.write_bytes(b"audio")
            empty = output / "empty.mp3"
            empty.touch()
            for path in [outside, empty, output / "missing.mp3"]:
                with self.assertRaises(bridge.BridgeError):
                    bridge.validate_outputs([{"file_path": str(path)}], output)
            good = output / "song.mp3"
            good.write_bytes(b"audio")
            self.assertEqual(bridge.validate_outputs([{"file_path": str(good)}], output), [str(good.resolve())])

    def test_raw_backend_errors_cannot_expose_tokens(self):
        secret = "session-token-should-not-leak"
        self.assertNotIn(secret, bridge.safe_message(RuntimeError(secret)))
        self.assertEqual(bridge.safe_message(bridge.BridgeError("Reconnect in Music settings")), "Reconnect in Music settings")

    def test_refresh_persists_without_erasing_other_services_or_newer_logins(self):
        spotify = {"service": "spotify", "uuid": "s", "login": {"refresh": "old"}}
        tidal = {"service": "tidal", "uuid": "t", "login": {"refresh": "old"}}
        updated = {**tidal, "login": {"refresh": "refreshed"}}
        merged = bridge.merge_accounts([spotify, tidal], [updated], [spotify, tidal], "tidal")
        self.assertIn(spotify, merged)
        self.assertIn(updated, merged)
        reconnected = {**tidal, "login": {"refresh": "new-interactive-login"}}
        merged = bridge.merge_accounts([spotify, tidal], [updated], [spotify, reconnected], "tidal")
        self.assertIn(reconnected, merged)
        self.assertNotIn(updated, merged)
        # Editing Spotify in the account window must retain a concurrent Tidal refresh.
        new_spotify = {**spotify, "login": {"refresh": "new-spotify"}}
        merged = bridge.merge_accounts([spotify, tidal], [new_spotify, tidal], [spotify, updated])
        self.assertIn(new_spotify, merged)
        self.assertIn(updated, merged)

    def test_idg_spotify_pairing_replaces_only_after_valid_session(self):
        old = {"service": "spotify", "uuid": "old", "active": True}
        other = {"service": "tidal", "uuid": "other"}
        class Config:
            accounts = [old, other]
            def get(self, key): return self.accounts
            def set(self, key, value): self.accounts = value
            def save(self): pass
        config = Config()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            class Server:
                closed = False
                def has_valid_session(self): return True
                def close_session(self): pass
                def close(self): self.closed = True
            server = Server()
            class Builder:
                def __init__(self): self.conf = SimpleNamespace()
                def create(self):
                    self_test.assertEqual(self.device_name, "IDG")
                    Path(self.conf.stored_credentials_file).write_text(json.dumps({"username": "fixture", "credentials": "fixture-secret", "type": "AUTHENTICATION_STORED_SPOTIFY_CREDENTIALS"}))
                    return server
            self_test = self
            zeroconf = SimpleNamespace(Builder=Builder, _ZeroconfServer__default_get_info_fields={})
            modules = {"librespot.zeroconf": SimpleNamespace(ZeroconfServer=zeroconf),
                       "onthespot.otsconfig": SimpleNamespace(config=config)}
            with patch.dict(sys.modules, modules):
                messages = []
                bridge.pair_spotify(root, messages.append)
                self.assertIn("select IDG", messages[0])
                self.assertEqual(zeroconf._ZeroconfServer__default_get_info_fields["brandDisplayName"], "IDG")
                self.assertTrue(server.closed)
                self.assertFalse((root / "spotify-session.json").exists())
                self.assertIn(other, config.accounts)
                self.assertNotIn(old, config.accounts)
                self.assertEqual(config.accounts[-1]["login"]["username"], "fixture")
                for cancelled, timeout in [(threading.Event(), 0), (threading.Event(), 1)]:
                    cancelled.set()
                    config.accounts = [old, other]
                    with self.assertRaises(bridge.BridgeError):
                        bridge.pair_spotify(root, lambda _: None, cancelled, timeout)
                    self.assertEqual(config.accounts, [old, other])
                    self.assertFalse((root / "spotify-session.json").exists())

    def test_idg_account_setup_rolls_back_failure_and_cancellation(self):
        old = {"service": "apple_music", "uuid": "old", "active": True}
        other = {"service": "tidal", "uuid": "other"}
        class Config:
            accounts = [old, other]
            def get(self, key): return self.accounts
            def set(self, key, value): self.accounts = value
            def save(self): pass
        config = Config()
        cancelled = threading.Event()
        def add_account(value):
            config.accounts.append({"service": "apple_music", "uuid": "new", "active": True})
            if value == "cancel": cancelled.set()
            if value == "fail": raise RuntimeError("private-token")
        modules = {"onthespot.otsconfig": SimpleNamespace(config=config),
                   "onthespot.api.apple_music": SimpleNamespace(apple_music_add_account=add_account)}
        with patch.dict(sys.modules, modules):
            for value in ["", "fail", "cancel"]:
                config.accounts = [old, other]
                cancelled.clear()
                with self.assertRaises(Exception):
                    bridge._configure_account("apple_music", [value], lambda _: None, cancelled)
                self.assertEqual(config.accounts, [old, other])
            cancelled.clear()
            bridge._configure_account("apple_music", ["success"], lambda _: None, cancelled)
            self.assertIn(other, config.accounts)
            self.assertNotIn(old, config.accounts)
            self.assertEqual(config.accounts[-1]["uuid"], "new")

    @unittest.skipUnless(os.name == "nt", "Windows DPAPI")
    def test_accounts_survive_new_reads_and_are_encrypted_at_rest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            accounts = [{"service": "spotify", "active": True, "login": {"credentials": "fixture-secret"}}]
            bridge.save_accounts(root, accounts)
            self.assertNotIn(b"fixture-secret", (root / "accounts.bin").read_bytes())
            self.assertEqual(bridge.load_accounts(root), accounts)
            code = f"import importlib.util; from pathlib import Path; s=importlib.util.spec_from_file_location('b',{str(spec.origin)!r}); b=importlib.util.module_from_spec(s); s.loader.exec_module(b); print(len(b.load_accounts(Path({str(root)!r}))))"
            restarted = subprocess.check_output([sys.executable, "-c", code], text=True).strip()
            self.assertEqual(restarted, "1")
            self.assertTrue(next(a for a in bridge.account_status(root) if a["service"] == "spotify")["configured"])
            bridge.save_accounts(root, [])
            self.assertEqual(bridge.load_accounts(root), [])

    @unittest.skipUnless(os.name == "nt", "Windows process leases")
    def test_cleanup_preserves_live_profiles_and_removes_only_exited_worker_profiles(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            active = root / "profile-active"
            active.mkdir()
            (active / "owner.json").write_text(json.dumps({"pid": os.getpid()}))
            process = subprocess.Popen([sys.executable, "-c", "pass"])
            process.wait(timeout=10)
            stale = root / "profile-stale"
            stale.mkdir()
            (stale / "owner.json").write_text(json.dumps({"pid": process.pid}))
            unrelated = root / "my-files"
            unrelated.mkdir()
            bridge.cleanup_stale_profiles(root)
            self.assertTrue(active.exists())
            self.assertFalse(stale.exists())
            self.assertTrue(unrelated.exists())


@unittest.skipUnless(importlib.util.find_spec("PyQt6"), "Optional music runtime required for UI checks")
class MusicAccountWindowTests(unittest.TestCase):
    def test_idg_window_masks_credentials_handles_async_errors_and_saves_no_secrets_in_ui(self):
        from PyQt6.QtWidgets import QApplication, QLineEdit
        from PyQt6.QtGui import QFont
        import time
        app = QApplication.instance() or QApplication([])
        app.setFont(QFont("Segoe UI", 10))
        class Config:
            accounts = [{"service": "tidal", "active": True, "uuid": "fixture"}]
            def get(self, key): return self.accounts
            def set(self, key, value): self.accounts = value
            def save(self): pass
        config = Config()
        with patch.dict(sys.modules, {"onthespot.otsconfig": SimpleNamespace(config=config)}):
            window = bridge.create_account_window()
            window.show()
            app.processEvents()
            try:
                self.assertEqual(window.windowTitle(), "IDG Music Accounts")
                self.assertFalse(any(name.startswith("onthespot.qt") for name in sys.modules))
                self.assertIn("select IDG", window.help.text())
                self.assertFalse(window.connect_button.isEnabled())
                window.service.setCurrentIndex(4)
                self.assertTrue(window.first.isVisible())
                self.assertTrue(window.second.isVisible())
                self.assertEqual(window.second.echoMode(), QLineEdit.EchoMode.Password)
                app.processEvents()
                if os.environ.get("IDG_MUSIC_UI_SCREENSHOT"):
                    window.grab().save(os.environ["IDG_MUSIC_UI_SCREENSHOT"])
                window.service.setCurrentIndex(2)
                self.assertFalse(window.second.isVisible())
                self.assertEqual(window.first.echoMode(), QLineEdit.EchoMode.Password)
                secret = "private-fixture-not-for-status"
                window.first.setText(secret)
                def fail(*args):
                    time.sleep(0.03)
                    raise RuntimeError(secret)
                with patch.object(bridge, "configure_account", fail):
                    window.connect_service()
                    self.assertFalse(window.service.isEnabled())
                    self.assertEqual(window.first.text(), "")
                    window.accept()  # Busy close requests cancellation instead of destroying a QThread.
                    self.assertTrue(window.task.cancelled.is_set())
                    deadline = time.monotonic() + 3
                    while window.task.isRunning() and time.monotonic() < deadline:
                        app.processEvents()
                        time.sleep(0.01)
                    self.assertFalse(window.task.isRunning())
                    app.processEvents()
                    self.assertTrue(window.service.isEnabled())
                    self.assertNotIn(secret, window.status.text())
                    self.assertIn("failed", window.status.text())
                self.assertEqual(config.accounts, [{"service": "tidal", "active": True, "uuid": "fixture"}])
            finally:
                window.close()
                app.processEvents()


if __name__ == "__main__":
    unittest.main()
