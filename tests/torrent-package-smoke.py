"""Transfer verified BitTorrent v1, v2 and hybrid pieces using the packaged worker.

Run with tools/torrent/python.exe -I tests/torrent-package-smoke.py.
All torrents and their peers are created locally for this test.
"""
import http.server
import json
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import parse_qs, urlsplit

import libtorrent as lt


def wait_for(predicate, timeout=20):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if predicate():
            return
        time.sleep(0.05)
    raise AssertionError("Timed out waiting for the local torrent seed")


def run_worker(config, expected, cancel=False):
    worker = Path(sys.executable).with_name("worker.py")
    with tempfile.TemporaryFile(mode="w+", encoding="utf-8") as transcript:
        with subprocess.Popen(
            [sys.executable, "-I", "-X", "utf8", "-u", str(worker)],
            stdin=subprocess.PIPE, stdout=transcript, stderr=subprocess.STDOUT,
            encoding="utf-8",
        ) as proc:
            proc.stdin.write(json.dumps(config) + "\n")
            proc.stdin.flush()
            if cancel:
                proc.stdin.close()
            try:
                proc.wait(timeout=30)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
                transcript.seek(0)
                raise AssertionError(transcript.read()[-3000:]) from None
            transcript.seek(0)
            text = transcript.read()
            assert proc.returncode == 0, text
            events = [json.loads(line) for line in text.splitlines()]
            if cancel:
                assert not any(event["event"] == "complete" for event in events), events
            else:
                assert events[-1] == {"event": "complete", "total": expected}, events


def smoke(root):
    seed = lt.session({"listen_interfaces": "127.0.0.1:0", "enable_dht": False,
                       "enable_lsd": False, "enable_upnp": False, "enable_natpmp": False})
    port = seed.listen_port()

    class Tracker(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            requester = parse_qs(urlsplit(self.path).query).get("port", ["0"])[0]
            peers = b"" if requester == str(port) else socket.inet_aton("127.0.0.1") + struct.pack("!H", port)
            reply = lt.bencode({b"interval": 60, b"peers": peers})
            self.send_response(200)
            self.send_header("Content-Length", str(len(reply)))
            self.end_headers()
            self.wfile.write(reply)

        def log_message(self, *_):
            pass

    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Tracker) as tracker:
        threading.Thread(target=tracker.serve_forever, daemon=True).start()
        try:
            for name, flags, magnet in (
                ("v1-single", lt.create_torrent.v1_only, False),
                ("v2-multi", lt.create_torrent.v2_only, True),
                ("hybrid-multi", 0, False),
            ):
                source_dir = root / "seed"
                source_dir.mkdir(exist_ok=True)
                if name.endswith("single"):
                    payloads = {Path(name + ".bin"): bytes(range(251)) * 256}
                else:
                    payloads = {Path(name) / "one.bin": bytes(range(251)) * 256,
                                Path(name) / "بيانات.bin": b"second torrent file" * 885}
                files = []
                for rel, data in payloads.items():
                    (source_dir / rel).parent.mkdir(parents=True, exist_ok=True)
                    (source_dir / rel).write_bytes(data)
                    files.append(lt.create_file_entry(rel.as_posix(), len(data)))
                torrent = lt.create_torrent(files, 16384, flags)
                torrent.set_priv(not magnet)
                torrent.add_tracker(f"http://127.0.0.1:{tracker.server_port}/announce")
                lt.set_piece_hashes(torrent, str(source_dir))
                metainfo = root / f"{name}.torrent"
                metainfo.write_bytes(lt.bencode(torrent.generate()))
                info = lt.torrent_info(str(metainfo))
                params = lt.load_torrent_file(str(metainfo))
                params.save_path = str(source_dir)
                params.flags &= ~lt.torrent_flags.auto_managed
                handle = seed.add_torrent(params)
                handle.resume()
                wait_for(lambda: handle.status().is_seeding)
                output = root / name
                output.mkdir()
                source = ({"magnet": lt.make_magnet_uri(params) + f"&x.pe=127.0.0.1:{port}"}
                          if magnet else {"file": str(metainfo)})
                run_worker({"source": source, "output": str(output), "download_limit": 0},
                           sum(map(len, payloads.values())))
                for rel, expected in payloads.items():
                    assert (output / rel).read_bytes() == expected, rel
                print(f"PASS {name} ({'magnet' if magnet else '.torrent'}): verified byte-for-byte")
                seed.remove_torrent(handle)
        finally:
            tracker.shutdown()

    run_worker({"source": {"magnet": "magnet:?xt=urn:btih:" + "a0" * 20},
                "output": str(root / "cancelled"), "download_limit": 0}, 0, cancel=True)
    print("PASS cancellation through parent pipe")


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="idg-torrent-smoke-") as directory:
        smoke(Path(directory))
