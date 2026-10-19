param([string]$AppDirectory)
if (-not $AppDirectory) {
    $repositoryRoot = Split-Path $PSScriptRoot -Parent
    $AppDirectory = if (Test-Path -LiteralPath (Join-Path $repositoryRoot 'IDG.exe')) { $repositoryRoot } else { Join-Path $repositoryRoot 'target/release' }
}
$ErrorActionPreference = 'Stop'
$appPath = (Resolve-Path -LiteralPath $AppDirectory).Path
$bridgePath = Join-Path $appPath 'IDG-native.exe'
foreach ($binary in @('IDG.exe', 'IDG-native.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $appPath $binary))) { throw "Build or copy $binary into $appPath first." }
}
$manifestPath = Join-Path (Split-Path $PSScriptRoot -Parent) 'extension/com.rust_idm.host.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$manifest.path = $bridgePath
$hostDirectory = Join-Path $env:APPDATA 'IDG'
New-Item -ItemType Directory -Path $hostDirectory -Force | Out-Null
$hostPath = Join-Path $hostDirectory 'com.rust_idm.host.json'
[IO.File]::WriteAllText($hostPath, ($manifest | ConvertTo-Json -Depth 4), [Text.UTF8Encoding]::new($false))
foreach ($browserKey in @('Google/Chrome', 'Microsoft/Edge')) {
    $keyPath = "HKCU:/Software/$browserKey/NativeMessagingHosts/com.rust_idm.host"
    New-Item -Path $keyPath -Force | Out-Null
    Set-Item -LiteralPath $keyPath -Value $hostPath
}
Write-Host "Registered IDG browser integration for this Windows user. Reload the IDG extension."
