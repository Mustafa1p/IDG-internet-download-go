"""Run an isolated Inno upgrade against a fixture that hides on Close, like IDG.
Never installs IDG or touches its registry, downloads, startup, or running process.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
ISCC = next((p for p in [Path(os.environ.get("LOCALAPPDATA", "")) / "Programs/Inno Setup 6/ISCC.exe", Path("C:/Program Files (x86)/Inno Setup 6/ISCC.exe"), Path("C:/Program Files/Inno Setup 6/ISCC.exe")] if p.is_file()), None)
CSC = Path(os.environ["WINDIR"]) / "Microsoft.NET/Framework64/v4.0.30319/csc.exe"
if not CSC.exists(): CSC = Path(os.environ["WINDIR"]) / "Microsoft.NET/Framework/v4.0.30319/csc.exe"
assert ISCC and CSC.is_file(), "Inno Setup and .NET Framework compiler are required"
FLAGS = subprocess.CREATE_NO_WINDOW
with tempfile.TemporaryDirectory(prefix="idg-install-close-") as temporary:
    directory = Path(temporary)
    installed = directory / "installed"
    installed.mkdir()
    source = directory / "fixture.cs"
    binary = directory / "IDG-close-fixture.exe"
    for version in [1, 2]:
        source.write_text(r'''
using System;
using System.IO;
using System.Reflection;
using System.Windows.Forms;
[assembly: AssemblyVersion("VERSION.0.0.0")]
class Fixture : Form {
    Fixture() {
        Text = "IDG isolated installer fixture";
        ShowInTaskbar = false;
        Shown += delegate { Hide(); File.WriteAllText(Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "ready"), "yes"); };
        FormClosing += delegate(object sender, FormClosingEventArgs e) { e.Cancel = true; Hide(); };
    }
    [STAThread] static void Main() { Application.Run(new Fixture()); }
}
'''.replace("VERSION", str(version)), encoding="utf-8")
        subprocess.run([str(CSC), "/nologo", "/target:winexe", "/reference:System.Windows.Forms.dll", "/out:" + str(binary), str(source)], check=True, capture_output=True, creationflags=FLAGS)
        if version == 1: shutil.copy2(binary, installed / binary.name)
    script = directory / "fixture.iss"
    script.write_text(f'''
[Setup]
AppName=IDG isolated close fixture
AppVersion=2.0
AppId=IDG-isolated-installer-close-fixture
DefaultDirName={installed}
OutputDir={directory}
OutputBaseFilename=fixture-setup
Uninstallable=no
CreateUninstallRegKey=no
PrivilegesRequired=lowest
UsePreviousAppDir=no
DirExistsWarning=no
CloseApplications=force
RestartApplications=no
CloseApplicationsFilter=IDG-close-fixture.exe
[Files]
Source: "{binary}"; DestDir: "{{app}}"; Flags: ignoreversion
''', encoding="utf-8")
    subprocess.run([str(ISCC), "/Q", str(script)], check=True, capture_output=True, creationflags=FLAGS)
    fixture = subprocess.Popen([str(installed / binary.name)], creationflags=FLAGS)
    try:
        deadline = time.monotonic() + 10
        while not (installed / "ready").exists():
            assert fixture.poll() is None
            assert time.monotonic() < deadline
            time.sleep(0.05)
        started = time.monotonic()
        result = subprocess.run([str(directory / "fixture-setup.exe"), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/LOG=" + str(directory / "setup.log"), "/DIR=" + str(installed)], capture_output=True, timeout=90, creationflags=FLAGS)
        assert result.returncode == 0, (result.returncode, (directory / "setup.log").read_text(errors="replace"))
        fixture.wait(timeout=10)
        assert (installed / binary.name).read_bytes() == binary.read_bytes()
        print(f"Installer replaced the locked fixture and exited successfully in {time.monotonic()-started:.1f}s; close-to-tray could not stall the upgrade.")
    finally:
        if fixture.poll() is None: fixture.kill(); fixture.wait(timeout=10)
