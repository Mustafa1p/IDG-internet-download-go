"""Exercise update handover using disposable executables, without installing IDG."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "target/release/IDG-native.exe"
FLAGS = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0


def wait_for(path, process=None):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if path.is_file():
            return
        if process is not None and process.poll() is not None:
            raise AssertionError(f"Update helper exited early: {process.returncode}")
        time.sleep(0.02)
    raise AssertionError(f"Timed out waiting for {path.name}")


with tempfile.TemporaryDirectory(prefix="IDG-update-test-") as temporary:
    directory = Path(temporary)
    source = directory / "fixture.rs"
    source.write_text(r'''
use std::{env,fs,thread,time::Duration};
fn main() {
    let executable = env::current_exe().unwrap();
    let directory = executable.parent().unwrap();
    match executable.file_stem().unwrap().to_str().unwrap() {
        "parent" => while !directory.join("release-parent").exists() { thread::sleep(Duration::from_millis(20)); },
        "setup" => fs::write(directory.join("installer-args"), env::args().skip(1).collect::<Vec<_>>().join("\n")).unwrap(),
        "IDG" => fs::write(directory.join("reopened"), "yes").unwrap(),
        _ => panic!("Unexpected fixture name"),
    }
}
''', encoding="utf-8")
    subprocess.run([shutil.which("rustc"), "--crate-name", "update_fixture", str(source), "-o", str(directory / "parent.exe")], check=True, creationflags=FLAGS)
    for name in ["setup.exe", "IDG.exe"]:
        shutil.copy2(directory / "parent.exe", directory / name)
    shutil.copy2(BINARY, directory / "helper.exe")
    parent = subprocess.Popen([str(directory / "parent.exe")], creationflags=FLAGS)
    helper = None
    try:
        plan = directory / "plan.json"
        plan.write_text(json.dumps({"directory": str(directory), "installer": str(directory / "setup.exe"), "parent_pid": parent.pid, "startup": True}), encoding="utf-8")
        helper = subprocess.Popen([str(directory / "helper.exe"), "--apply-update", str(plan)], creationflags=FLAGS)
        wait_for(plan.with_suffix(".ready"), helper)
        assert not (directory / "installer-args").exists(), "Installer must wait for IDG to exit"
        (directory / "release-parent").touch()
        parent.wait(timeout=15)
        helper.wait(timeout=15)
        assert helper.returncode == 0
        wait_for(directory / "reopened")
        args = (directory / "installer-args").read_text().splitlines()
        assert f"/DIR={directory}" in args
        assert "/TASKS=startupicon" in args
        assert not (directory / "setup.exe").exists(), "Successful installer should be removed"
        time.sleep(0.2)  # Allow the disposable reopened fixture to release its executable.
        print("Update helper waited for exit, preserved install/startup arguments, reopened IDG, and cleaned the installer")
    finally:
        for process in [helper, parent]:
            if process is not None and process.poll() is None:
                process.kill()
                process.wait()
