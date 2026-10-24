<# Sign final Windows files, or verify them without accessing signing credentials.
   Azure Artifact Signing uses IDG_SIGNING_DLIB and IDG_SIGNING_METADATA.
   Local hardware/certificate-store signing uses IDG_SIGNING_THUMBPRINT.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string[]]$Path,
    [switch]$VerifyOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$signTool = $env:IDG_SIGNTOOL
if (-not $signTool) {
    $signTool = Get-ChildItem "${env:ProgramFiles(x86)}/Windows Kits/10/bin/*/x64/signtool.exe" -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $signTool -or -not (Test-Path -LiteralPath $signTool -PathType Leaf)) {
    throw 'Install Windows SDK x64 SignTool, or set IDG_SIGNTOOL.'
}

$files = @($Path | ForEach-Object {
    $item = Get-Item -LiteralPath $_
    if ($item.PSIsContainer) {
        Get-ChildItem -LiteralPath $item.FullName -Recurse -File |
            Where-Object Extension -In '.exe', '.dll', '.ps1'
    } else { $item }
} | Sort-Object FullName -Unique)
if ($files.Count -eq 0) { throw 'No executable files found to sign or verify.' }

foreach ($file in $files) {
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if (-not $VerifyOnly -and $signature.Status -ne 'Valid') {
        # Never hide a modified or otherwise broken upstream signature by replacing it.
        if ($signature.Status -ne 'NotSigned') {
            throw "Invalid existing signature on $($file.FullName): $($signature.Status)"
        }
        $arguments = @('sign', '/fd', 'SHA256', '/td', 'SHA256')
        if ($env:IDG_SIGNING_DLIB -and $env:IDG_SIGNING_METADATA) {
            $arguments += @('/tr', 'http://timestamp.acs.microsoft.com', '/dlib', $env:IDG_SIGNING_DLIB, '/dmdf', $env:IDG_SIGNING_METADATA)
        } elseif ($env:IDG_SIGNING_THUMBPRINT) {
            if (-not $env:IDG_TIMESTAMP_URL) { throw 'Set IDG_TIMESTAMP_URL to your certificate provider RFC3161 timestamp service.' }
            $arguments += @('/tr', $env:IDG_TIMESTAMP_URL, '/sha1', $env:IDG_SIGNING_THUMBPRINT)
        } else {
            throw 'No signing identity configured. Configure signing or build without -SignRelease; see docs/windows-signing.md.'
        }
        & $signTool @arguments $file.FullName
        if ($LASTEXITCODE -ne 0) { throw "SignTool signing failed: $($file.FullName)" }
        $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    }
    if ($signature.Status -ne 'Valid' -or -not $signature.SignerCertificate -or -not $signature.TimeStamperCertificate) {
        throw "A trusted, timestamped signature is required: $($file.FullName) ($($signature.Status))"
    }
    & $signTool verify /pa /all /tw $file.FullName
    if ($LASTEXITCODE -ne 0) { throw "SignTool verification failed: $($file.FullName)" }
    Write-Host "Verified $($file.Name): $($signature.SignerCertificate.Subject)"
}
