"""Run one BitTorrent transfer in the embedded Windows runtime."""
import json
import os
import sys
import threading
from pathlib import PureWindowsPath

import libtorrent as lt


def report(event, **fields):
    print(json.dumps({"event": event, **fields}), flush=True)


def check_paths(info):
    files = info.layout()
    paths = set()
    for index in range(files.num_files()):
        if files.file_flags(index) & lt.file_storage.flag_pad_file:
            continue
        path = PureWindowsPath(files.file_path(index))
        if (path.anchor or not path.parts or files.file_flags(index) & lt.file_storage.flag_symlink
                or any(part in (".", "..") or ":" in part or part.rstrip(" .") != part
                       or PureWindowsPath(part).is_reserved() for part in path.parts)):
            raise ValueError("The torrent has a file path that is unsafe on Windows.")
        key = tuple(part.casefold() for part in path.parts)
        if key in paths:
            raise ValueError("Torrent paths collide on Windows.")
        paths.add(key)
    if any(key[:size] in paths for key in paths for size in range(1, len(key))):
        raise ValueError("A torrent file shares a path with a directory.")


def transfer(config, stopped):
    source = config["source"]
    if "magnet" in source:
        params = lt.parse_magnet_uri(source["magnet"])
    else:
        params = lt.load_torrent_file(source["file"])
        check_paths(params.ti)

    params.save_path = config["output"]
    params.download_limit = config["download_limit"]
    # Fetch magnet metadata before enabling payload requests and disk writes.
    params.flags |= lt.torrent_flags.upload_mode
    params.flags &= ~lt.torrent_flags.auto_managed
    session = lt.session({"listen_interfaces": "0.0.0.0:0,[::]:0"})
    handle = session.add_torrent(params)
    handle.resume()
    validated = False
    while not stopped.is_set():
        status = handle.status()
        if status.errc.value():
            raise RuntimeError(status.errc.message())
        if status.has_metadata:
            if not validated:
                check_paths(handle.torrent_file())
                handle.unset_flags(lt.torrent_flags.upload_mode)
                validated = True
            if status.is_finished:
                total = status.total_wanted
                # Close the session before IDG moves the folder on Windows.
                session.remove_torrent(handle)
                return total
            report("progress", done=status.total_wanted_done, total=status.total_wanted,
                   rate=status.download_payload_rate)
        stopped.wait(0.25)
    return None


def main():
    config = json.loads(sys.stdin.readline())
    stopped = threading.Event()

    def watch_parent():
        os.read(0, 1)
        stopped.set()

    threading.Thread(target=watch_parent, daemon=True).start()
    total = transfer(config, stopped)
    if total is not None:
        report("complete", total=total)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError) as exc:
        report("error", message=f"BitTorrent: {exc}")
        sys.exit(1)
