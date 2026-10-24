#!/usr/bin/env pwsh
<#
.SYNOPSIS
  Build IDG Windows installer.
  Downloads Inno Setup if not present, then compiles installer\setup.iss
.EXAMPLE
  .\installer\build-installer.ps1
#>
param([switch]$SignRelease, [ValidateSet("x64", "x86")][string]$Architecture = "x64")
$buildDir = if ($Architecture -eq "x86") { "target/i686-pc-windows-msvc/release" } else { "target/release" }
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

# ── 1. Build release binary ──────────────────────────────────────────────────
Write-Host "`n[1/3] Building IDG release binary..." -ForegroundColor Cyan
Push-Location $root
if ($Architecture -eq "x86") {
    rustup target add i686-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "x86 Rust target unavailable" }
    cargo build --locked --release --bins --target i686-pc-windows-msvc
} else { cargo build --locked --release --bins }
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
Pop-Location
python "$root/installer/fetch-media-tools.py" $Architecture
if ($LASTEXITCODE -ne 0) { throw "media tools download failed" }
if ($SignRelease) {
    & "$PSScriptRoot/sign-windows.ps1" -Path "$root/$buildDir/IDG.exe", "$root/$buildDir/IDG-native.exe", "$root/$buildDir/tools", "$PSScriptRoot/register-native-host.ps1"
} else {
    Write-Warning 'Unsigned Windows installer. Windows may display an unknown-publisher or SmartScreen warning. Use -SignRelease to enable trusted signing; see docs/windows-signing.md.'
}

# ── 2. Ensure Inno Setup 6 ───────────────────────────────────────────────────
$iscc = @(
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe"
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $iscc) {
    Write-Host "`n[2/3] Inno Setup not found – downloading portable version..." -ForegroundColor Yellow

    $innoInstaller = "$env:TEMP\innosetup.exe"
    $innoUrl = "https://jrsoftware.org/download.php/is.exe"
    Write-Host "     Downloading from $innoUrl"
    Invoke-WebRequest -Uri $innoUrl -OutFile $innoInstaller -UseBasicParsing
    if ((Get-AuthenticodeSignature -LiteralPath $innoInstaller).Status -ne 'Valid') {
        throw 'The downloaded Inno Setup installer does not have a valid trusted signature.'
    }

    Write-Host "     Installing Inno Setup silently..."
    Start-Process -FilePath $innoInstaller -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART" -WindowStyle Hidden -Wait
    Remove-Item $innoInstaller -Force

    $iscc = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe"
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1

    if (-not $iscc) { throw "Inno Setup installation failed – please install manually from https://jrsoftware.org/isinfo.php" }
} else {
    Write-Host "`n[2/3] Inno Setup found at: $iscc" -ForegroundColor Green
}

# ── 3. Compile installer ──────────────────────────────────────────────────────
Write-Host "`n[3/3] Compiling installer..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force "$root\dist" | Out-Null

$compilerArgs = @("$root\installer\setup.iss")
if ($Architecture -eq "x86") {
    $compilerArgs = @("/DPackageArch=x86", "/DPackageSuffix=-x86", "/DBuildDir=..\target\i686-pc-windows-msvc\release") + $compilerArgs
}
if ($SignRelease) {
    $signCommand = 'pwsh.exe -NoProfile -NonInteractive -File $q' + "$PSScriptRoot/sign-windows.ps1" + '$q -Path $f'
    $compilerArgs = @('/DIDG_SIGN_RELEASE', "/Sidg=$signCommand") + $compilerArgs
}
& $iscc @compilerArgs
if ($LASTEXITCODE -ne 0) { throw "ISCC compilation failed" }

$output = Get-ChildItem "$root\dist\IDG-Setup-*.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($output) {
    if ($SignRelease) { & "$PSScriptRoot/sign-windows.ps1" -VerifyOnly -Path $output.FullName }
    $sizeMB = [math]::Round($output.Length / 1MB, 2)
    Write-Host "`n✅ Installer built: $($output.FullName) ($sizeMB MB)" -ForegroundColor Green
} else {
    Write-Host "`n⚠  Installer file not found in dist\" -ForegroundColor Yellow
}

