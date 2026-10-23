# Prepare the official Artifact Signing client for the release job.
$ErrorActionPreference = 'Stop'
foreach ($name in 'IDG_SIGNING_ENDPOINT', 'IDG_SIGNING_ACCOUNT', 'IDG_SIGNING_PROFILE') {
    if (-not [Environment]::GetEnvironmentVariable($name)) { throw "Missing $name. See docs/windows-signing.md." }
}
$clientRoot = Join-Path $env:RUNNER_TEMP 'idg-signing-client'
$null = New-Item -ItemType Directory -Force -Path $clientRoot
& nuget install Microsoft.ArtifactSigning.Client -Version 1.0.146 -OutputDirectory $clientRoot -NonInteractive -DirectDownload
if ($LASTEXITCODE -ne 0) { throw 'Could not install the Artifact Signing client.' }
$dlib = Join-Path $clientRoot 'Microsoft.ArtifactSigning.Client.1.0.146/bin/x64/Azure.CodeSigning.Dlib.dll'
if (-not (Test-Path -LiteralPath $dlib)) { throw 'Artifact Signing x64 library missing.' }
$metadata = Join-Path $clientRoot 'metadata.json'
@{
    Endpoint = $env:IDG_SIGNING_ENDPOINT
    CodeSigningAccountName = $env:IDG_SIGNING_ACCOUNT
    CertificateProfileName = $env:IDG_SIGNING_PROFILE
    # azure/login supplies Azure CLI credentials via GitHub OIDC; no long-lived key.
    ExcludeCredentials = @('EnvironmentCredential', 'WorkloadIdentityCredential', 'ManagedIdentityCredential',
        'SharedTokenCacheCredential', 'VisualStudioCredential', 'VisualStudioCodeCredential',
        'AzurePowerShellCredential', 'AzureDeveloperCliCredential', 'InteractiveBrowserCredential')
} | ConvertTo-Json | Set-Content -LiteralPath $metadata -Encoding utf8
"IDG_SIGNING_DLIB=$dlib" >> $env:GITHUB_ENV
"IDG_SIGNING_METADATA=$metadata" >> $env:GITHUB_ENV
