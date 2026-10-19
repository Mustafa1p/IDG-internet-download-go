<div align="center">

# IDG — Internet Download GO

**High-speed, multi-segment download manager for Windows**

Built in Rust · Native UI · Browser Extension · MIT License

[![Build](https://img.shields.io/github/actions/workflow/status/mustafa1p/IDG-internet-download-go/release.yml?branch=main&style=flat-square&logo=github)](https://github.com/mustafa1p/IDG-internet-download-go/actions)
[![Release](https://img.shields.io/github/v/release/mustafa1p/IDG-internet-download-go?style=flat-square&color=0bc&label=latest)](https://github.com/mustafa1p/IDG-internet-download-go/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078d4?style=flat-square&logo=windows)](https://github.com/mustafa1p/IDG-internet-download-go/releases)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square&logo=rust)](https://rustup.rs)
[![License](https://img.shields.io/badge/license-MIT-22c55e?style=flat-square)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen?style=flat-square)](CONTRIBUTING.md)

[**Download Installer**](https://github.com/mustafa1p/IDG-internet-download-go/releases/latest) · [Report Bug](https://github.com/mustafa1p/IDG-internet-download-go/issues) · [Request Feature](https://github.com/mustafa1p/IDG-internet-download-go/issues)

---

</div>

## What is IDG?

IDG is a desktop download manager that accelerates file downloads by splitting each file into multiple parallel segments — the same strategy used by IDM (Internet Download Manager), rebuilt from scratch in Rust with a clean, native UI.

**Instead of one connection downloading 1 MB/s, IDG opens 8–16 connections and downloads at 8–16 MB/s** (network limit permitting).

---

## Screenshot

![IDG Screenshot](docs/screenshot.png)

> Dark, compact interface. Toolbar with icon buttons · Category sidebar · Multi-column download table · Live speed and ETA.

---

## Features

- **Parallel HTTP segments** — splits every file into 1–32 simultaneous `Range: bytes=X-Y` connections  
- **Pre-allocated files** — disk space is reserved before downloading; prevents fragmentation  
- **Smart resume** — each segment resumes from its last byte on retry; no re-downloading  
- **Auto filename detection** — reads `Content-Disposition` header or falls back to URL path  
- **Browser extension** — Chrome / Edge MV3 extension intercepts downloads automatically  
- **Context-menu downloads** — right-click any link/video in the browser → *Download with IDG*  
- **Native messaging** — extension communicates with IDG via Chrome's native messaging protocol  
- **Live progress** — 4-second rolling speed average updated 4× per second  
- **ETA** — estimated time remaining per download  
- **Pause / Resume / Cancel** — per-download atomic flags respected by every worker chunk  
- **Category filter** — All · Downloading · Finished · Paused · Failed  
- **Persistent settings** — default save folder and segment count saved across sessions  
- **Single EXE** — 8 MB, no runtime, no installer required for portable use  

---

## Download & Install

### Recommended — Windows Installer

Download `IDG-Setup-x.x.x.exe` from the [latest release](https://github.com/mustafa1p/IDG-internet-download-go/releases/latest).

The installer will:
- Install IDG to `%ProgramFiles%\IDG`
- Optionally create a Desktop shortcut
- Optionally start IDG with Windows
- Register the native messaging host for Chrome and Edge automatically

### Portable

Download `IDG.exe` and run it directly — no installation needed.

---

## Browser Extension Setup

> The installer handles steps 3–4 automatically.

1. Open `chrome://extensions` → enable **Developer mode**
2. Click **Load unpacked** → select the `extension/` folder (or the installed copy at `%ProgramFiles%\IDG\extension`)
3. Copy the **Extension ID** shown on the card
4. Open `extension/com.rust_idm.host.json`, set `"path"` to your IDG.exe path and `"allowed_origins"` to your extension ID
5. Register the native host once (run in PowerShell as your user):

```powershell
$json = "$env:ProgramFiles\IDG\extension\com.rust_idm.host.json"

# Chrome
reg add "HKCU\Software\Google\Chrome\NativeMessagingHosts\com.rust_idm.host" `
    /ve /t REG_SZ /d $json /f

# Edge
reg add "HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.rust_idm.host" `
    /ve /t REG_SZ /d $json /f
```

---

## How it works

IDG uses the same multi-segment architecture reverse-engineered from IDM:

```
                      ┌─────────────────────────────────────────────┐
URL ──── HEAD probe ──┤ Content-Length: 1 073 741 824               │
                      │ Accept-Ranges: bytes                        │
                      └─────────────────┬───────────────────────────┘
                                        │  supports_multipart = true
                      ┌─────────────────▼───────────────────────────┐
                      │  Pre-allocate  output.file  (1 GB)          │
                      └─────────────────┬───────────────────────────┘
                                        │
             ┌──────────────────────────┼──────────────────────────┐
             │                          │                          │
     Segment 0                  Segment 1 … N-1             Segment N
  Range: bytes=0-134M      Range: bytes=134M-268M    Range: bytes=…-1G
       │                          │                          │
  write @ offset 0          write @ 134 MB              write @ end
       │                          │                          │
       └──────────────────────────┴──────────────────────────┘
                                  │
                       ✔  File complete (no copy needed)
```

**Evidence from IDMan.exe binary strings:**

| String | Meaning |
|---|---|
| `Range: bytes=%I64d-%I64d` | Parallel segment range format |
| `Range: bytes=%I64d-` | Open-ended final segment |
| `ConnectionsNumber` | Configurable parallel connection count |
| `Connection is restarted...` | Per-segment retry / resume logic |
| `Ka=["127.0.0.1:1001"]` *(extension source)* | IDM listens on localhost:1001 |

---

## Project structure

```
IDG-internet-download-go/
│
├── src/
│   ├── main.rs                   # Entry point: runtime + eframe bootstrap
│   ├── app.rs                    # Full GUI (toolbar, sidebar, table, dialogs)
│   └── download/
│       ├── mod.rs                # Module declarations
│       ├── types.rs              # DownloadItem, Status, Downloads
│       ├── probe.rs              # HEAD probe → size + Accept-Ranges
│       ├── segment.rs            # Range: bytes=X-Y worker (seek+write)
│       └── engine.rs             # Orchestrator: probe → split → spawn
│
├── extension/
│   ├── manifest.json             # Chrome Manifest V3
│   ├── background.js             # webRequest interceptor + native messaging
│   ├── content.js                # Download-button overlay on media elements
│   ├── popup.html / popup.js     # Extension popup
│   ├── com.rust_idm.host.json    # Native messaging host registration
│   └── images/                  # Icons: 16, 32, 48, 128 px
│
├── installer/
│   ├── setup.iss                 # Inno Setup 6 script
│   └── build-installer.ps1       # Auto-download Inno Setup + compile
│
├── .github/workflows/
│   └── release.yml               # CI: build + GitHub Release on tag push
│
├── Cargo.toml
├── CONTRIBUTING.md
├── CHANGELOG.md
├── LICENSE
└── README.md
```

---

## Build from source

### Prerequisites

| Tool | Version | Install |
|---|---|---|
| Rust | 1.75+ | `winget install Rustlang.Rustup` |
| Visual Studio Build Tools | 2022 | Required for `ring` (TLS) |

### Steps

```powershell
git clone https://github.com/mustafa1p/IDG-internet-download-go.git
cd IDG

# Debug build (fast compile, unoptimised)
cargo build

# Release build (optimised, ~8 MB, stripped)
cargo build --release
# → target\release\IDG.exe
```

### Build the installer

```powershell
.\installer\build-installer.ps1
# Downloads Inno Setup automatically if not installed
# → dist\IDG-Setup-1.0.0.exe
```

---

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) first.

```powershell
cargo clippy -- -D warnings   # zero warnings required
cargo fmt                     # rustfmt formatting required
cargo test                    # all tests must pass
```

---

## License

[MIT](LICENSE) © 2026 mustafa1p

