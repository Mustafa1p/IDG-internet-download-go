#!/usr/bin/env pwsh
<#
.SYNOPSIS
  Build IDG Windows installer.
  Downloads Inno Setup if not present, then compiles installer\setup.iss
.EXAMPLE
  .\installer\build-installer.ps1
#>
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

# ── 1. Build release binary ──────────────────────────────────────────────────
Write-Host "`n[1/3] Building IDG release binary..." -ForegroundColor Cyan
Push-Location $root
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
Pop-Location

# ── 2. Ensure Inno Setup 6 ───────────────────────────────────────────────────
$iscc = @(
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $iscc) {
    Write-Host "`n[2/3] Inno Setup not found – downloading portable version..." -ForegroundColor Yellow

    $innoInstaller = "$env:TEMP\innosetup.exe"
    $innoUrl = "https://jrsoftware.org/download.php/is.exe"
    Write-Host "     Downloading from $innoUrl"
    Invoke-WebRequest -Uri $innoUrl -OutFile $innoInstaller -UseBasicParsing

    Write-Host "     Installing Inno Setup silently..."
    Start-Process -FilePath $innoInstaller -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART" -Wait
    Remove-Item $innoInstaller -Force

    $iscc = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1

    if (-not $iscc) { throw "Inno Setup installation failed – please install manually from https://jrsoftware.org/isinfo.php" }
} else {
    Write-Host "`n[2/3] Inno Setup found at: $iscc" -ForegroundColor Green
}

# ── 3. Compile installer ──────────────────────────────────────────────────────
Write-Host "`n[3/3] Compiling installer..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force "$root\dist" | Out-Null

& $iscc "$root\installer\setup.iss"
if ($LASTEXITCODE -ne 0) { throw "ISCC compilation failed" }

$output = Get-ChildItem "$root\dist\IDG-Setup-*.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($output) {
    $sizeMB = [math]::Round($output.Length / 1MB, 2)
    Write-Host "`n✅ Installer built: $($output.FullName) ($sizeMB MB)" -ForegroundColor Green
} else {
    Write-Host "`n⚠  Installer file not found in dist\" -ForegroundColor Yellow
}

