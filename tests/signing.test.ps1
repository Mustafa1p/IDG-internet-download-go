# Exercise release failure boundaries without credentials or trusting a test CA.
$ErrorActionPreference = 'Stop'
$signer = Join-Path (Split-Path $PSScriptRoot -Parent) 'installer/sign-windows.ps1'
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('idg-signing-tests-' + [guid]::NewGuid())
$null = New-Item -ItemType Directory -Path "$fixture/payload [x]/tools" -Force
$savedEnv = @{}
foreach ($name in 'IDG_SIGNTOOL', 'IDG_SIGNING_DLIB', 'IDG_SIGNING_METADATA', 'IDG_SIGNING_THUMBPRINT', 'IDG_TIMESTAMP_URL') {
    $savedEnv[$name] = [Environment]::GetEnvironmentVariable($name)
    [Environment]::SetEnvironmentVariable($name, $null)
}
$global:IDGTestCalls = [Collections.Generic.List[string]]::new()
$global:IDGTestSignatures = @{}
$global:IDGTestExit = 0
function Get-AuthenticodeSignature {
    param([string]$LiteralPath)
    $state = $global:IDGTestSignatures[$LiteralPath]
    if (-not $state) { $state = 'NotSigned' }
    [pscustomobject]@{
        Status = $state
        SignerCertificate = if ($state -eq 'Valid' -or $state -eq 'NoTimestamp') { [pscustomobject]@{ Subject = 'CN=Fixture' } } else { $null }
        TimeStamperCertificate = if ($state -eq 'Valid') { [pscustomobject]@{ Subject = 'CN=Fixture TSA' } } else { $null }
    }
}
function Assert-Fails([scriptblock]$Action, [string]$Message) {
    try { & $Action } catch {
        if ($_.Exception.Message -like "*$Message*") { return }
        throw "Expected '$Message', received: $_"
    }
    throw "Expected failure: $Message"
}
function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
try {
    $mockTool = Join-Path $fixture 'mock-signtool.ps1'
    @'
$global:IDGTestCalls.Add($args[0])
if ($args[0] -eq 'sign' -and $global:IDGTestExit -eq 0) {
    $global:IDGTestSignatures[$args[-1]] = 'Valid'
}
$global:LASTEXITCODE = $global:IDGTestExit
'@ | Set-Content -LiteralPath $mockTool
    $env:IDG_SIGNTOOL = $mockTool
    $app = Join-Path $fixture 'payload [x]/app.exe'
    $dll = Join-Path $fixture 'payload [x]/tools/media.dll'
    Set-Content -LiteralPath $app -Value fixture
    Set-Content -LiteralPath $dll -Value fixture
    Assert-Fails { & $signer -Path $app } 'No signing identity configured'
    Assert-Fails { & $signer -VerifyOnly -Path $app } 'trusted, timestamped signature'
    Assert-True ($global:IDGTestCalls.Count -eq 0) 'Unsigned verification must not invoke signing.'

    $env:IDG_SIGNING_THUMBPRINT = 'fixture'
    Assert-Fails { & $signer -Path $app } 'IDG_TIMESTAMP_URL'
    $env:IDG_TIMESTAMP_URL = 'https://timestamp.example.test'
    $global:IDGTestSignatures[$app] = 'HashMismatch'
    Assert-Fails { & $signer -Path $app } 'Invalid existing signature'
    Assert-True ($global:IDGTestCalls.Count -eq 0) 'Broken upstream signatures must never be overwritten.'

    $global:IDGTestSignatures[$app] = 'NotSigned'
    & $signer -Path (Join-Path $fixture 'payload [x]')
    Assert-True (($global:IDGTestCalls | Where-Object { $_ -eq 'sign' }).Count -eq 2) 'App and nested DLL must both be signed.'
    $global:IDGTestCalls.Clear()
    & $signer -Path $app
    Assert-True ($global:IDGTestCalls.Count -eq 1 -and $global:IDGTestCalls[0] -eq 'verify') 'Preserve existing trusted signatures.'

    $global:IDGTestSignatures[$app] = 'NoTimestamp'
    Assert-Fails { & $signer -VerifyOnly -Path $app } 'trusted, timestamped signature'
    $global:IDGTestSignatures[$app] = 'Valid'
    $global:IDGTestExit = 1
    Assert-Fails { & $signer -VerifyOnly -Path $app } 'verification failed'
    $global:IDGTestSignatures[$app] = 'NotSigned'
    Assert-Fails { & $signer -Path $app } 'signing failed'
    $empty = New-Item -ItemType Directory -Path "$fixture/empty"
    Assert-Fails { & $signer -Path $empty.FullName } 'No executable files'
    Write-Host 'Signing regression tests passed (10 scenarios).'
} finally {
    foreach ($name in $savedEnv.Keys) { [Environment]::SetEnvironmentVariable($name, $savedEnv[$name]) }
    Remove-Item Function:\Get-AuthenticodeSignature
    Remove-Variable IDGTestCalls, IDGTestSignatures, IDGTestExit -Scope Global
    # Only delete the exact test directory created above inside the system temp folder.
    $resolved = [IO.Path]::GetFullPath($fixture)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
        (Split-Path $resolved -Leaf) -notlike 'idg-signing-tests-*') { throw 'Unexpected test cleanup path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
# The negative cases intentionally set a mock native failure exit code. GitHub's
# pwsh wrapper propagates LASTEXITCODE even when all assertions passed.
$global:LASTEXITCODE = 0
