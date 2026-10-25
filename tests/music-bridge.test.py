"""Music job completion and Windows credential persistence regressions."""
import importlib.util
import os
from pathlib import Path
import sys
import json
import subprocess
import tempfile
import unittest

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


if __name__ == "__main__":
    unittest.main()
