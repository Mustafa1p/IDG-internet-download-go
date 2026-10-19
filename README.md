<div align="center">

<br/>

<img src="extension/images/icon128.png" width="80" />

# IDG — Internet Download GO

**A fast, lightweight download manager for Windows.**  
Splits every file into multiple parallel connections — just like IDM, but free and open source.

<br/>

[![Build](https://img.shields.io/github/actions/workflow/status/Mustafa1p/IDG-internet-download-go/release.yml?branch=main&style=for-the-badge&logo=github&label=Build)](https://github.com/Mustafa1p/IDG-internet-download-go/actions)
[![Release](https://img.shields.io/github/v/release/Mustafa1p/IDG-internet-download-go?style=for-the-badge&color=0bc&label=Latest+Release)](https://github.com/Mustafa1p/IDG-internet-download-go/releases/latest)
[![License](https://img.shields.io/badge/License-MIT-22c55e?style=for-the-badge)](LICENSE)
[![Platform](https://img.shields.io/badge/Windows-10%20%2F%2011-0078d4?style=for-the-badge&logo=windows)](https://github.com/Mustafa1p/IDG-internet-download-go/releases/latest)

<br/>

## ⬇️ Download

<a href="https://github.com/Mustafa1p/IDG-internet-download-go/releases/latest/download/IDG-Setup-v1.0.0.exe">
  <img src="https://img.shields.io/badge/Download%20Installer-Setup%20.exe-1976d2?style=for-the-badge&logo=windows&logoColor=white" height="42"/>
</a>
&nbsp;&nbsp;
<a href="https://github.com/Mustafa1p/IDG-internet-download-go/releases/latest/download/IDG-extension-v1.0.0.zip">
  <img src="https://img.shields.io/badge/Download%20Extension-Chrome%20%2F%20Edge-F4B400?style=for-the-badge&logo=googlechrome&logoColor=white" height="42"/>
</a>

<br/><br/>

> **No registration. No ads. No limits.**

<br/>

</div>

---

## 📸 Screenshot

![IDG Screenshot](docs/screenshot.png)

---

## ✨ What is IDG?

**IDG (Internet Download GO)** is a free and open-source download manager for Windows.

It works the same way as **IDM (Internet Download Manager)** — it splits every download into multiple parallel connections and downloads all parts at the same time. This makes downloads **3–10× faster** than a regular single-connection download.

| Feature | IDG | Regular Browser |
|---|---|---|
| Parallel segments | ✅ Up to 32 | ❌ 1 |
| Pause & Resume | ✅ | ❌ |
| Browser integration | ✅ Chrome & Edge | — |
| Auto filename detect | ✅ | ❌ |
| Cost | **Free** | — |

---

## 🚀 How to install

### Step 1 — Download the installer

👉 **[Download IDG-Setup-v1.0.0.exe](https://github.com/Mustafa1p/IDG-internet-download-go/releases/latest/download/IDG-Setup-v1.0.0.exe)**

### Step 2 — Run the installer

Double-click the downloaded file and follow the wizard:

```
1. Click "Next"
2. Choose install folder  (default: C:\Program Files\IDG)
3. Check "Create Desktop shortcut" if you want one
4. Click "Install"
5. IDG opens automatically when done ✅
```

### Step 3 — Start downloading

- Click **⬇ Add URL** in the toolbar
- Paste any direct download link
- Click **OK** — IDG downloads it with multiple parallel connections

---

## 🔌 How to install the browser extension

The extension lets IDG **automatically intercept downloads** from Chrome and Edge.

### Step 1 — Download the extension

👉 **[Download IDG-extension-v1.0.0.zip](https://github.com/Mustafa1p/IDG-internet-download-go/releases/latest/download/IDG-extension-v1.0.0.zip)**

### Step 2 — Extract the ZIP

Right-click the downloaded `.zip` → **Extract All** → remember the folder location.

### Step 3 — Load in Chrome or Edge

1. Open Chrome → address bar → type `chrome://extensions` → Enter
2. Toggle **Developer mode** ON (top-right corner)
3. Click **Load unpacked**
4. Select the extracted folder
5. The IDG icon appears in your browser toolbar ✅

### Step 4 — Test it

Click any download link in your browser. IDG will automatically intercept it and start downloading with multiple parallel connections.

> **Tip:** Right-click any link or image → **Download with IDG**

---

## 🎯 Features

- **Multi-segment downloads** — splits files into 1–32 parallel connections
- **Pause & Resume** — stop and continue any download at any time
- **Auto filename** — automatically reads the correct filename from the server
- **Browser extension** — intercepts downloads from Chrome and Edge
- **Right-click menu** — right-click any link → "Download with IDG"
- **Live speed & ETA** — see real-time download speed and estimated time
- **Category filter** — view All / Downloading / Finished / Paused / Failed
- **Dark compact UI** — clean, minimal interface
- **Portable or installed** — works as a setup installer or a standalone EXE
- **Free & open source** — MIT license

---

## 🏗️ Build from source

```powershell
# Requirements: Rust 1.75+ and Visual Studio Build Tools 2022

git clone https://github.com/Mustafa1p/IDG-internet-download-go.git
cd IDG-internet-download-go

cargo build --release
# Output: target\release\IDG.exe
```

### Build the Windows installer

```powershell
.\installer\build-installer.ps1
# → dist\IDG-Setup-v1.0.0.exe
# (downloads Inno Setup automatically if not installed)
```

---

## 🤝 Contributing

Pull requests are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) first.

```powershell
cargo fmt
cargo clippy
cargo build --release
```

---

## 📄 License

[MIT](LICENSE) © 2026 [mustafa1p](https://github.com/Mustafa1p)
