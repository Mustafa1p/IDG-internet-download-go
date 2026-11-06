"""Run one benchmark client with durability timing and lifetime resource counters.

Hash verification belongs to the caller and stays outside this timed interval.
Resource queries also run outside it; no measurement polling thread is added.
"""
import os
from pathlib import Path
import subprocess
import time

from process_metrics import snapshot


def measure_client(command, destination, log_path, timeout, *, env=None):
    destination = Path(destination)
    started = time.perf_counter()
    with Path(log_path).open("wb") as log:
        with subprocess.Popen(command, stdout=log, stderr=log, env=env,
                              creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0) as client:
            try:
                try:
                    code = client.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    client.kill()
                    client.wait()
                    code = "timeout"
                process_seconds = time.perf_counter() - started
                sync_error = None
                if destination.is_file():
                    try:
                        with destination.open("r+b") as file:
                            os.fsync(file.fileno())
                    except OSError as error:
                        sync_error = str(error)
                durable_seconds = time.perf_counter() - started
                # Hold the owned process handle through these queries. A PID
                # lookup after exit could instead measure a reused PID.
                resources = snapshot(client)
            finally:
                if client.poll() is None:
                    client.kill()
                    client.wait()
    return dict(exit_code=code, process_seconds=process_seconds,
                durable_seconds=durable_seconds, sync_error=sync_error,
                resources=resources)
